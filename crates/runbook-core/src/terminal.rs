use crate::{
    model::{new_id, now},
    shell,
    store::check_revision,
    terminal_protocol::{self, Decoder},
    ApiError, Result, Store,
};
use base64::{engine::general_purpose::STANDARD, Engine};
#[cfg(not(windows))]
use portable_pty::ChildKiller;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    io::{Read, Write},
    sync::{Arc, Mutex},
    thread,
};

const OUTPUT_LIMIT: usize = 2 * 1024 * 1024;
const SESSION_LIMIT: usize = 32;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartRequest {
    pub project: String,
    pub command: Option<String>,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    pub if_revision: Option<u64>,
    pub cols: u16,
    pub rows: u16,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecuteRequest {
    pub session: String,
    pub project: String,
    pub command: String,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    pub if_revision: Option<u64>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionInfo {
    pub id: String,
    pub command_id: String,
    pub name: String,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub exit_code: Option<i64>,
    pub stopped: bool,
}
type Writer = Box<dyn Write + Send>;
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub project_id: String,
    pub command_id: Option<String>,
    pub name: String,
    pub cwd: String,
    pub started_at: u64,
    pub exit_code: Option<u32>,
    pub finished: bool,
    pub stopped: bool,
    pub error: Option<String>,
    pub interactive: bool,
    pub ready: bool,
    pub execution: Option<ExecutionInfo>,
}
struct Output {
    info: SessionInfo,
    chunks: VecDeque<(u64, Vec<u8>)>,
    next: u64,
    bytes: usize,
    input_dirty: bool,
}
struct Session {
    output: Arc<Mutex<Output>>,
    writer: Mutex<Option<Writer>>,
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    #[cfg(not(windows))]
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    #[cfg(windows)]
    job: job::Job,
}
#[derive(Clone, Default)]
pub struct TerminalManager {
    sessions: Arc<Mutex<HashMap<String, Arc<Session>>>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub session: SessionInfo,
    pub chunks: Vec<String>,
    pub next_sequence: u64,
    pub truncated: bool,
}

fn lock<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>> {
    mutex
        .lock()
        .map_err(|_| ApiError::new("TERMINAL_ERROR", "Terminal state lock was poisoned"))
}
fn pty_error(e: impl std::fmt::Display) -> ApiError {
    ApiError::new("TERMINAL_ERROR", e.to_string())
}
impl TerminalManager {
    pub fn start(&self, store: &Store, request: StartRequest) -> Result<SessionInfo> {
        let project = store.get(&request.project)?;
        if project.archived {
            return Err(ApiError::new(
                "PROJECT_ARCHIVED",
                "Restore the project before opening its terminal",
            ));
        }
        let (rendered, name) = if let Some(id) = request.command.as_deref() {
            let command = project
                .commands
                .iter()
                .find(|c| c.id == id)
                .ok_or_else(|| ApiError::not_found("command", id))?;
            check_revision(request.if_revision, command.revision)?;
            (
                Some(shell::render(command, &request.values)?),
                command.name.clone(),
            )
        } else {
            (None, "PowerShell".into())
        };
        self.spawn(
            &request.project,
            request.command.as_deref(),
            name,
            shell::prepare(&project.path, rendered)?,
            request.cols,
            request.rows,
            None,
        )
    }
    pub fn open_interactive(
        &self,
        store: &Store,
        project_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<SessionInfo> {
        let project = store.get(project_id)?;
        if project.archived {
            return Err(ApiError::new(
                "PROJECT_ARCHIVED",
                "Restore the project before opening its terminal",
            ));
        }
        if !cfg!(windows) {
            return Err(ApiError::new(
                "UNSUPPORTED",
                "Persistent desktop terminals currently require PowerShell on Windows",
            ));
        }
        let token = new_id("prompt");
        let mut shell = shell::prepare(&project.path, None)?;
        let script = terminal_protocol::bootstrap(&token);
        let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
        shell.args.extend([
            "-NoExit".into(),
            "-EncodedCommand".into(),
            STANDARD.encode(bytes),
        ]);
        self.spawn(
            project_id,
            None,
            "PowerShell".into(),
            shell,
            cols,
            rows,
            Some(token),
        )
    }
    pub fn execute(&self, store: &Store, request: ExecuteRequest) -> Result<SessionInfo> {
        let project = store.get(&request.project)?;
        if project.archived {
            return Err(ApiError::new(
                "PROJECT_ARCHIVED",
                "Restore the project before running a command",
            ));
        }
        let command = project
            .commands
            .iter()
            .find(|c| c.id == request.command)
            .ok_or_else(|| ApiError::not_found("command", &request.command))?;
        check_revision(request.if_revision, command.revision)?;
        let input = terminal_protocol::input(&shell::render(command, &request.values)?);
        if input.len() > 64 * 1024 {
            return Err(ApiError::field("data", "Terminal input exceeds 64 KiB"));
        }
        let session = self.get(&request.session)?;
        let mut writer = lock(&session.writer)?;
        let writer = writer
            .as_mut()
            .ok_or_else(|| ApiError::new("SESSION_FINISHED", "This terminal process has exited"))?;
        let mut output = lock(&session.output)?;
        if output.info.project_id != request.project {
            return Err(ApiError::new(
                "PROJECT_MISMATCH",
                "Terminal belongs to a different project",
            ));
        }
        if !output.info.interactive || output.info.finished {
            return Err(ApiError::new(
                "SESSION_FINISHED",
                "Open an interactive terminal before running a command",
            ));
        }
        if !output.info.ready {
            return Err(ApiError::new(
                "SESSION_BUSY",
                "终端正在运行程序或尚未准备好，请等待提示符后再执行。",
            ));
        }
        if output.input_dirty {
            return Err(ApiError::new(
                "TERMINAL_INPUT_PENDING",
                "终端中还有未提交的输入，请先执行或清空当前行。",
            ));
        }
        output.info.ready = false;
        output.info.execution = Some(ExecutionInfo {
            id: new_id("execution"),
            command_id: command.id.clone(),
            name: command.name.clone(),
            started_at: now(),
            finished_at: None,
            exit_code: None,
            stopped: false,
        });
        let info = output.info.clone();
        drop(output);
        if let Err(error) = writer
            .write_all(input.as_bytes())
            .and_then(|_| writer.flush())
        {
            lock(&session.output)?.info.error = Some(format!("Terminal input failed: {error}"));
            return Err(error.into());
        }
        Ok(info)
    }
    #[allow(clippy::too_many_arguments)]
    fn spawn(
        &self,
        project_id: &str,
        command_id: Option<&str>,
        name: String,
        shell: shell::ShellCommand,
        cols: u16,
        rows: u16,
        token: Option<String>,
    ) -> Result<SessionInfo> {
        let mut sessions = lock(&self.sessions)?;
        if sessions.len() >= SESSION_LIMIT {
            return Err(ApiError::new(
                "SESSION_LIMIT",
                "Close an existing terminal before opening another (limit 32)",
            ));
        }
        let size = terminal_size(cols, rows)?;
        let pair = native_pty_system().openpty(size).map_err(pty_error)?;
        let mut cmd = CommandBuilder::new(&shell.program);
        cmd.args(&shell.args);
        cmd.cwd(&shell.cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        let mut child = pair.slave.spawn_command(cmd).map_err(pty_error)?;
        #[cfg(windows)]
        let job = match job::Job::assign(child.process_id()) {
            Ok(job) => job,
            Err(e) => {
                let _ = child.kill();
                return Err(e);
            }
        };
        #[cfg(not(windows))]
        let killer = child.clone_killer();
        let reader = pair.master.try_clone_reader().map_err(pty_error)?;
        let writer = pair.master.take_writer().map_err(pty_error)?;
        drop(pair.slave);
        let info = SessionInfo {
            id: new_id("session"),
            project_id: project_id.into(),
            command_id: command_id.map(str::to_owned),
            name,
            cwd: shell.cwd,
            started_at: now(),
            exit_code: None,
            finished: false,
            stopped: false,
            error: None,
            interactive: token.is_some(),
            ready: false,
            execution: None,
        };
        let output = Arc::new(Mutex::new(Output {
            info: info.clone(),
            chunks: VecDeque::new(),
            next: 0,
            bytes: 0,
            input_dirty: false,
        }));
        let session = Arc::new(Session {
            output: output.clone(),
            writer: Mutex::new(Some(writer)),
            master: Mutex::new(Some(pair.master)),
            #[cfg(not(windows))]
            killer: Mutex::new(killer),
            #[cfg(windows)]
            job,
        });
        sessions.insert(info.id.clone(), session.clone());
        // Reader owns a duplicate pipe. The waiter drains it before publishing finished=true.
        let reader_output = output.clone();
        let reader_thread = thread::spawn(move || read_output(reader, reader_output, token));
        thread::spawn(move || {
            let status = child.wait();
            drop(child);
            // Closing the input and PTY job on exit also releases descendant-held console handles.
            if let Ok(mut writer) = session.writer.lock() {
                writer.take();
            }
            #[cfg(windows)]
            let _ = session.job.terminate();
            if let Ok(mut master) = session.master.lock() {
                master.take();
            }
            let _ = reader_thread.join();
            if let Ok(mut state) = output.lock() {
                match status {
                    Ok(status) => state.info.exit_code = Some(status.exit_code()),
                    Err(e) => state.info.error = Some(e.to_string()),
                }
                state.info.finished = true;
                state.info.ready = false;
                let exit_code = state.info.exit_code.map(i64::from);
                if let Some(execution) = state
                    .info
                    .execution
                    .as_mut()
                    .filter(|e| e.finished_at.is_none())
                {
                    execution.finished_at = Some(now());
                    execution.exit_code = exit_code;
                }
            }
        });
        Ok(info)
    }
    fn get(&self, id: &str) -> Result<Arc<Session>> {
        lock(&self.sessions)?
            .get(id)
            .cloned()
            .ok_or_else(|| ApiError::not_found("session", id))
    }
    pub fn list(&self, project_id: Option<&str>) -> Result<Vec<SessionInfo>> {
        let sessions = lock(&self.sessions)?;
        let mut result = vec![];
        for session in sessions.values() {
            let info = lock(&session.output)?.info.clone();
            if project_id.is_none_or(|p| info.project_id == p) {
                result.push(info);
            }
        }
        result.sort_by_key(|s| s.started_at);
        Ok(result)
    }
    pub fn snapshot(&self, id: &str, after: u64) -> Result<Snapshot> {
        let session = self.get(id)?;
        let output = lock(&session.output)?;
        let oldest = output.chunks.front().map(|v| v.0).unwrap_or(output.next);
        Ok(Snapshot {
            session: output.info.clone(),
            chunks: output
                .chunks
                .iter()
                .filter(|(seq, _)| *seq >= after)
                .map(|(_, bytes)| STANDARD.encode(bytes))
                .collect(),
            next_sequence: output.next,
            truncated: after < oldest,
        })
    }
    pub fn write(&self, id: &str, data: &str) -> Result<()> {
        if data.len() > 65536 {
            return Err(ApiError::field("data", "Terminal input exceeds 64 KiB"));
        }
        let session = self.get(id)?;
        let mut writer = lock(&session.writer)?;
        let writer = writer
            .as_mut()
            .ok_or_else(|| ApiError::new("SESSION_FINISHED", "This terminal process has exited"))?;
        let mut output = lock(&session.output)?;
        if output.info.interactive {
            if data.contains(['\r', '\n']) {
                output.input_dirty = false;
                output.info.ready = false;
            } else if data.contains('\u{3}') {
                output.input_dirty = false;
            } else if !data.is_empty()
                && (!data.starts_with('\u{1b}') || data.starts_with("\u{1b}[200~"))
                && output.info.ready
            {
                output.input_dirty = true;
            }
        }
        drop(output);
        writer.write_all(data.as_bytes())?;
        writer.flush()?;
        Ok(())
    }
    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<()> {
        let session = self.get(id)?;
        let master = lock(&session.master)?;
        if let Some(master) = master.as_ref() {
            master.resize(terminal_size(cols, rows)?).map_err(pty_error)
        } else {
            Ok(())
        }
    }
    pub fn stop(&self, id: &str, force: bool) -> Result<()> {
        let session = self.get(id)?;
        {
            let mut output = lock(&session.output)?;
            if output.info.finished {
                return Ok(());
            }
            output.info.stopped = true;
            if let Some(execution) = output
                .info
                .execution
                .as_mut()
                .filter(|e| e.finished_at.is_none())
            {
                execution.stopped = true;
            }
        }
        if force {
            #[cfg(windows)]
            session.job.terminate()?;
            #[cfg(not(windows))]
            lock(&session.killer)?.kill().map_err(pty_error)?;
        } else {
            self.write(id, "\u{3}")?;
        }
        Ok(())
    }
    pub fn close(&self, id: &str) -> Result<()> {
        self.stop(id, true)?;
        // Release the console synchronously, including when the app is about to exit.
        // The waiter may not get another turn before process shutdown.
        let session = self.get(id)?;
        lock(&session.writer)?.take();
        lock(&session.master)?.take();
        lock(&self.sessions)?.remove(id);
        Ok(())
    }
    pub fn close_all(&self) {
        if let Ok(list) = self.list(None) {
            for info in list {
                let _ = self.close(&info.id);
            }
        }
    }
}
fn terminal_size(cols: u16, rows: u16) -> Result<PtySize> {
    if !(2..=1000).contains(&cols) || !(2..=1000).contains(&rows) {
        return Err(ApiError::field(
            "size",
            "Terminal rows/columns must be between 2 and 1000",
        ));
    }
    Ok(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })
}
fn read_output(
    mut reader: Box<dyn Read + Send>,
    output: Arc<Mutex<Output>>,
    token: Option<String>,
) {
    let mut decoder = token.as_deref().map(Decoder::new);
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                if let Ok(mut state) = output.lock() {
                    let (visible, prompts) = if let Some(decoder) = decoder.as_mut() {
                        decoder.feed(&buffer[..n])
                    } else {
                        (buffer[..n].to_vec(), vec![])
                    };
                    for prompt in prompts {
                        state.info.ready = true;
                        state.input_dirty = false;
                        state.info.cwd = prompt.cwd;
                        if let Some(execution) = state
                            .info
                            .execution
                            .as_mut()
                            .filter(|e| e.finished_at.is_none())
                        {
                            execution.finished_at = Some(now());
                            execution.exit_code = Some(if execution.stopped {
                                130
                            } else {
                                prompt.exit_code
                            });
                        }
                    }
                    if !visible.is_empty() {
                        let seq = state.next;
                        state.next += 1;
                        state.bytes += visible.len();
                        state.chunks.push_back((seq, visible));
                    }
                    while state.bytes > OUTPUT_LIMIT {
                        if let Some((_, old)) = state.chunks.pop_front() {
                            state.bytes -= old.len();
                        } else {
                            break;
                        }
                    }
                } else {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    if let Some(decoder) = decoder.as_mut() {
        let rest = decoder.finish();
        if !rest.is_empty() {
            if let Ok(mut state) = output.lock() {
                let seq = state.next;
                state.next += 1;
                state.bytes += rest.len();
                state.chunks.push_back((seq, rest));
            }
        }
    }
}

#[cfg(windows)]
mod job {
    use super::*;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::{JobObjects::*, Threading::*};
    pub struct Job(OwnedHandle);
    impl Job {
        pub fn assign(pid: Option<u32>) -> Result<Self> {
            let pid = pid.ok_or_else(|| {
                ApiError::new("TERMINAL_ERROR", "PTY did not expose a process ID")
            })?;
            // SAFETY: all handles are checked for null and immediately placed under RAII ownership.
            unsafe {
                let raw = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if raw.is_null() {
                    return Err(std::io::Error::last_os_error().into());
                }
                let job = Self(OwnedHandle::from_raw_handle(raw));
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                if SetInformationJobObject(
                    raw,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const _,
                    std::mem::size_of_val(&limits) as u32,
                ) == 0
                {
                    return Err(std::io::Error::last_os_error().into());
                }
                let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
                if process.is_null() {
                    return Err(std::io::Error::last_os_error().into());
                }
                let process = OwnedHandle::from_raw_handle(process);
                if AssignProcessToJobObject(raw, process.as_raw_handle()) == 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
                Ok(job)
            }
        }
        pub fn terminate(&self) -> Result<()> {
            // SAFETY: the owned handle remains valid for the duration of the call.
            unsafe {
                if TerminateJobObject(self.0.as_raw_handle(), 130) == 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
            Ok(())
        }
    }
}
