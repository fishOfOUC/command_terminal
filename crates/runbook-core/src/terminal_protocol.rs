// Private PowerShell prompt frames distinguish a returned prompt from process termination.
use base64::{engine::general_purpose::STANDARD, Engine};

pub struct Prompt {
    pub exit_code: i64,
    pub cwd: String,
}
pub struct Decoder {
    prefix: Vec<u8>,
    pending: Vec<u8>,
}
impl Decoder {
    pub fn new(token: &str) -> Self {
        Self {
            prefix: format!("\x1b]777;runbook;{token};").into_bytes(),
            pending: vec![],
        }
    }
    pub fn feed(&mut self, bytes: &[u8]) -> (Vec<u8>, Vec<Prompt>) {
        self.pending.extend_from_slice(bytes);
        let mut visible = vec![];
        let mut prompts = vec![];
        loop {
            if let Some(start) = self
                .pending
                .windows(self.prefix.len())
                .position(|s| s == self.prefix)
            {
                visible.extend(self.pending.drain(..start));
                let Some(end) = self.pending.iter().position(|b| *b == 7) else {
                    if self.pending.len() > 16384 {
                        visible.append(&mut self.pending);
                    }
                    break;
                };
                let frame: Vec<_> = self.pending.drain(..=end).collect();
                let parsed = std::str::from_utf8(&frame[self.prefix.len()..frame.len() - 1])
                    .ok()
                    .and_then(|s| s.split_once(';'))
                    .and_then(|(code, cwd)| {
                        Some(Prompt {
                            exit_code: code.parse().ok()?,
                            cwd: String::from_utf8(STANDARD.decode(cwd).ok()?).ok()?,
                        })
                    });
                if let Some(prompt) = parsed {
                    prompts.push(prompt);
                } else {
                    visible.extend(frame);
                }
            } else {
                let keep = (1..self.prefix.len())
                    .rev()
                    .find(|n| self.pending.ends_with(&self.prefix[..*n]))
                    .unwrap_or(0);
                visible.extend(self.pending.drain(..self.pending.len() - keep));
                break;
            }
        }
        (visible, prompts)
    }
    pub fn finish(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending)
    }
}

pub fn bootstrap(token: &str) -> String {
    format!(
        r#"[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false); $OutputEncoding=[Console]::OutputEncoding; function global:prompt {{ $rbOk=$?; $rbCode=if($null -ne $global:__RunbookExecutionExitCode){{$global:__RunbookExecutionExitCode}}elseif($rbOk){{0}}elseif($global:LASTEXITCODE){{$global:LASTEXITCODE}}else{{1}}; $global:__RunbookExecutionExitCode=$null; $rbCwd=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($executionContext.SessionState.Path.CurrentLocation.Path)); [Console]::Write(([string][char]27)+']777;runbook;{token};'+$rbCode+';'+$rbCwd+([string][char]7)); 'PS '+$executionContext.SessionState.Path.CurrentLocation+'> ' }}"#
    )
}
pub fn input(line: &str) -> String {
    // Dot-source so Set-Location, variables and functions belong to this terminal, not a child scope.
    // A newline before the closing brace also handles a trailing PowerShell comment.
    format!(
        ". {{ $global:LASTEXITCODE = 0; $global:__RunbookExecutionExitCode = 130;\rtry {{\r{}\r$global:__RunbookExecutionExitCode = if ($?) {{ $global:LASTEXITCODE }} elseif ($global:LASTEXITCODE) {{ $global:LASTEXITCODE }} else {{ 1 }}\r}} catch {{ $global:__RunbookExecutionExitCode = 1; Write-Error -ErrorRecord $_ -ErrorAction Continue }}\r}}\r",
        line.replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\n', "\r")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frames_survive_arbitrary_pipe_boundaries_and_keep_utf8_output() {
        let cwd = "E:\\项目目录";
        let frame = format!(
            "输出\x1b]777;runbook;secret;7;{}\x07PS > ",
            STANDARD.encode(cwd)
        );
        for width in 1..frame.len() {
            let mut decoder = Decoder::new("secret");
            let mut text = vec![];
            let mut events = vec![];
            for chunk in frame.as_bytes().chunks(width) {
                let (v, p) = decoder.feed(chunk);
                text.extend(v);
                events.extend(p);
            }
            text.extend(decoder.finish());
            assert_eq!(String::from_utf8(text).unwrap(), "输出PS > ");
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].cwd, cwd);
            assert_eq!(events[0].exit_code, 7);
        }
    }
    #[test]
    fn foreign_invalid_and_incomplete_frames_are_not_lost() {
        let text = b"before\x1b]777;runbook;other;0;QQ==\x07after";
        let mut decoder = Decoder::new("secret");
        assert_eq!(decoder.feed(text).0, text);
        let invalid = b"\x1b]777;runbook;secret;bad\x07";
        assert_eq!(decoder.feed(invalid).0, invalid);
        let incomplete = b"\x1b]777;runbook;secret;0;";
        assert!(decoder.feed(incomplete).0.is_empty());
        assert_eq!(decoder.finish(), incomplete);
    }
    #[test]
    fn command_block_keeps_multiline_content_and_trailing_comments() {
        let script = input("$value = 3\nWrite-Output $value # note");
        assert!(script.contains(
            "$value = 3\rWrite-Output $value # note\r$global:__RunbookExecutionExitCode"
        ));
        assert!(script.ends_with("}\r"));
    }
    #[cfg(windows)]
    #[test]
    fn powershell_script_preserves_state_and_recovers_after_failure_without_a_pty() {
        use std::os::windows::process::CommandExt;
        let work = tempfile::tempdir().unwrap();
        let shell = crate::shell::prepare(&work.path().to_string_lossy(), None).unwrap();
        let commands = [
            format!(
                "$rbTestValue='retained'; Set-Location -LiteralPath {}",
                crate::shell::quote(&shell.cwd)
            ),
            "& $env:ComSpec /d /c 'exit 7'".into(),
            "Write-Output \"RB_STATE:$rbTestValue\"".into(),
            "throw 'expected-script-error'".into(),
            "Write-Output 'RB_RECOVERED'".into(),
        ];
        let mut script = bootstrap("script-check") + "\n";
        for command in commands {
            script.push_str(&input(&command).replace('\r', "\n"));
            script.push_str("prompt\n");
        }
        let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let output = std::process::Command::new(shell.program)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
                &STANDARD.encode(bytes),
            ])
            .creation_flags(0x08000000)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut decoder = Decoder::new("script-check");
        let (visible, prompts) = decoder.feed(&output.stdout);
        assert_eq!(
            prompts.iter().map(|p| p.exit_code).collect::<Vec<_>>(),
            vec![0, 7, 0, 1, 0]
        );
        assert!(prompts.iter().all(|p| p.cwd == shell.cwd));
        let text = String::from_utf8_lossy(&visible);
        assert!(text.contains("RB_STATE:retained"));
        assert!(text.contains("RB_RECOVERED"));
    }
}
