use crate::{
    install::{self, Plan, STATE},
    manifest::{self, MAX_PACKAGE},
    package, Manifest,
};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    pub phase: String,
    pub available: Option<Manifest>,
    pub downloaded: u64,
    pub total: u64,
    pub message: String,
    pub previous_result: Option<String>,
}
struct Inner {
    status: UpdateStatus,
    signed: Option<Vec<u8>>,
    prepared: Option<PathBuf>,
}
pub struct Controller {
    app: PathBuf,
    inner: Mutex<Inner>,
    operation: Mutex<()>,
}

impl Controller {
    pub fn new(app: PathBuf) -> Self {
        let controller = Self {
            app,
            operation: Mutex::new(()),
            inner: Mutex::new(Inner {
                status: UpdateStatus {
                    current_version: crate::CURRENT_VERSION.into(),
                    phase: "idle".into(),
                    available: None,
                    downloaded: 0,
                    total: 0,
                    message: "可检查新版本".into(),
                    previous_result: None,
                },
                signed: None,
                prepared: None,
            }),
        };
        controller.restore_download();
        controller
    }
    fn restore_download(&self) {
        let result = (|| -> Result<()> {
            let stage: PathBuf =
                serde_json::from_slice(&fs::read(self.app.join(STATE).join("prepared.json"))?)?;
            ensure!(
                stage.parent() == Some(self.app.join(STATE).as_path())
                    && stage
                        .file_name()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| s.starts_with("download-")),
                "缓存路径无效"
            );
            let signed = fs::read(stage.join("manifest.json"))?;
            let manifest = crate::verify_manifest(&signed, crate::PUBLIC_KEY)?;
            ensure!(manifest::newer(&manifest.version)?, "缓存版本已过期");
            package::verify_archive(&stage.join("package.zip"), &manifest)?;
            let mut inner = self.inner.lock().unwrap();
            inner.status.total = manifest.size;
            inner.status.downloaded = manifest.size;
            inner.status.available = Some(manifest);
            inner.signed = Some(signed);
            inner.prepared = Some(stage);
            inner.status.phase = "ready".into();
            inner.status.message = "已恢复下载完成的更新，可重启更新".into();
            Ok(())
        })();
        let _ = result; // An expired or incomplete cache is safe to ignore and download again.
    }
    pub fn status(&self) -> UpdateStatus {
        let mut status = self.inner.lock().unwrap().status.clone();
        status.previous_result = fs::read(self.app.join(STATE).join("last-result.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        status
    }
    fn phase(&self, phase: &str, message: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.status.phase = phase.into();
        inner.status.message = message.into();
    }
    pub fn check(&self) -> Result<UpdateStatus> {
        let _guard = self
            .operation
            .try_lock()
            .map_err(|_| anyhow::anyhow!("更新操作正在进行"))?;
        if self.inner.lock().unwrap().prepared.is_some() {
            return Ok(self.status());
        }
        self.phase("checking", "正在检查更新…");
        let result = manifest::fetch_manifest().and_then(|(m, bytes)| {
            let newer = manifest::newer(&m.version)?;
            let mut inner = self.inner.lock().unwrap();
            inner.status.phase = if newer { "available" } else { "current" }.into();
            inner.status.message = if newer {
                format!("发现新版本 {}", m.version)
            } else {
                "当前已是最新版本".into()
            };
            inner.status.available = newer.then_some(m);
            inner.signed = newer.then_some(bytes);
            Ok(())
        });
        if let Err(error) = result {
            self.phase("error", &format!("检查失败：{error:#}"));
        }
        Ok(self.status())
    }
    pub fn download(&self) -> Result<UpdateStatus> {
        let _guard = self
            .operation
            .try_lock()
            .map_err(|_| anyhow::anyhow!("更新操作正在进行"))?;
        let (m, signed) = {
            let inner = self.inner.lock().unwrap();
            ensure!(inner.prepared.is_none(), "更新已下载完成");
            (
                inner.status.available.clone().context("请先检查更新")?,
                inner.signed.clone().context("缺少已签名清单")?,
            )
        };
        self.phase("downloading", "正在下载更新…");
        {
            let mut i = self.inner.lock().unwrap();
            i.status.downloaded = 0;
            i.status.total = m.size;
        }
        let stage =
            install::state_dir(&self.app)?.join(format!("download-{}", uuid::Uuid::new_v4()));
        let result = (|| -> Result<()> {
            fs::create_dir(&stage)?;
            let mut response = manifest::client(600)?
                .get(&m.url)
                .send()?
                .error_for_status()?;
            if let Some(size) = response.content_length() {
                ensure!(size == m.size, "下载长度与签名清单不一致");
            }
            let mut file = File::create(stage.join("package.zip"))?;
            let mut chunk = [0; 64 * 1024];
            let mut downloaded = 0u64;
            loop {
                let n = response.read(&mut chunk)?;
                if n == 0 {
                    break;
                }
                downloaded += n as u64;
                ensure!(
                    downloaded <= m.size && downloaded <= MAX_PACKAGE,
                    "下载超出预期大小"
                );
                file.write_all(&chunk[..n])?;
                self.inner.lock().unwrap().status.downloaded = downloaded;
            }
            file.sync_all()?;
            drop(file);
            package::verify_archive(&stage.join("package.zip"), &m)?;
            package::extract(&stage.join("package.zip"), &stage.join("validation"))?;
            fs::remove_dir_all(stage.join("validation"))?;
            fs::write(stage.join("manifest.json"), signed)?;
            fs::copy(
                self.app.join("runbook-updater.exe"),
                stage.join("helper.exe"),
            )
            .context("缺少便携更新器，请使用完整发布包")?;
            install::write_json(&self.app.join(STATE).join("prepared.json"), &stage)?;
            self.inner.lock().unwrap().prepared = Some(stage.clone());
            self.phase("ready", "下载并验证完成，可重启更新");
            Ok(())
        })();
        if let Err(error) = result {
            self.phase("error", &format!("下载失败：{error:#}"));
            let _ = fs::remove_dir_all(stage);
        }
        Ok(self.status())
    }
    pub fn install(&self, data: &Path, project: Option<String>) -> Result<()> {
        let _guard = self
            .operation
            .try_lock()
            .map_err(|_| anyhow::anyhow!("更新操作正在进行"))?;
        let stage = self
            .inner
            .lock()
            .unwrap()
            .prepared
            .clone()
            .context("请先下载并验证更新")?;
        let plan = Plan {
            app: self.app.clone(),
            stage: stage.clone(),
            data: dunce::canonicalize(data)?,
            project,
            token: uuid::Uuid::new_v4().to_string(),
        };
        install::write_json(&stage.join("plan.json"), &plan)?;
        let mut command = Command::new(stage.join("helper.exe"));
        command
            .arg("--plan")
            .arg(stage.join("plan.json"))
            .current_dir(&self.app);
        let mut child = install::spawn_detached(&mut command)?;
        let start = std::time::Instant::now();
        loop {
            if fs::read(stage.join("accepted.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<String>(&b).ok())
                .as_deref()
                == Some(&plan.token)
            {
                break;
            }
            ensure!(
                child.try_wait()?.is_none(),
                "更新器启动失败，请查看上次更新结果"
            );
            if start.elapsed() > std::time::Duration::from_secs(30) {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("更新器未就绪，应用保持打开，请稍后重试");
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        self.phase("installing", "正在退出并安装更新…");
        Ok(())
    }
}
