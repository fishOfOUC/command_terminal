use anyhow::{bail, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, VerifyingKey};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{io::Read, time::Duration};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const ENDPOINT: &str =
    "https://github.com/fishOfOUC/command_terminal/releases/latest/download/latest.json";
pub const MAX_PACKAGE: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub app: String,
    pub version: String,
    pub platform: String,
    pub data_schema: u32,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub notes: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    payload: String,
    signature: String,
}

pub fn verify_manifest(bytes: &[u8], public_key: &str) -> Result<Manifest> {
    ensure!(bytes.len() <= 128 * 1024, "更新清单过大");
    let envelope: Envelope = serde_json::from_slice(bytes).context("更新清单格式错误")?;
    let key: [u8; 32] = STANDARD
        .decode(public_key.trim())?
        .try_into()
        .map_err(|_| anyhow::anyhow!("更新公钥格式错误"))?;
    let signature = Signature::from_slice(&STANDARD.decode(envelope.signature)?)?;
    VerifyingKey::from_bytes(&key)?
        .verify_strict(envelope.payload.as_bytes(), &signature)
        .context("更新签名验证失败，已拒绝此更新")?;
    let m: Manifest = serde_json::from_str(&envelope.payload)?;
    ensure!(
        m.schema == 1 && m.app == "dev.runbook.desktop",
        "更新协议或应用不匹配"
    );
    ensure!(
        m.platform == "windows-x86_64" && cfg!(all(windows, target_arch = "x86_64")),
        "更新包不支持当前平台"
    );
    ensure!(
        m.data_schema == 1,
        "此更新需要数据迁移，请手动升级并备份数据库"
    );
    let v = Version::parse(&m.version).context("更新版本号无效")?;
    ensure!(
        v.pre.is_empty() && v.build.is_empty(),
        "自动更新只接受正式版本"
    );
    ensure!(m.size > 0 && m.size <= MAX_PACKAGE, "更新包大小无效");
    ensure!(
        m.sha256.len() == 64 && m.sha256.bytes().all(|c| c.is_ascii_hexdigit()),
        "更新包校验值无效"
    );
    ensure!(m.notes.len() <= 32 * 1024, "更新说明过长");
    validate_url(&m.url)?;
    Ok(m)
}

pub(crate) fn validate_url(url: &str) -> Result<()> {
    let u = reqwest::Url::parse(url)?;
    let local_debug = cfg!(debug_assertions)
        && u.scheme() == "http"
        && matches!(u.host_str(), Some("127.0.0.1" | "localhost"));
    ensure!(
        u.scheme() == "https" || local_debug,
        "更新仅允许 HTTPS 下载"
    );
    ensure!(
        u.username().is_empty() && u.password().is_none(),
        "更新地址不能包含凭据"
    );
    Ok(())
}

pub(crate) fn client(timeout: u64) -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(concat!("Runbook/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(timeout))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || validate_url(attempt.url().as_str()).is_err() {
                attempt.error("更新重定向不安全或次数过多")
            } else {
                attempt.follow()
            }
        }))
        .build()?)
}

pub(crate) fn fetch_manifest() -> Result<(Manifest, Vec<u8>)> {
    let endpoint = if cfg!(debug_assertions) {
        std::env::var("RUNBOOK_UPDATE_ENDPOINT").unwrap_or_else(|_| ENDPOINT.into())
    } else {
        ENDPOINT.into()
    };
    validate_url(&endpoint)?;
    let response = client(30)?
        .get(endpoint)
        .send()
        .context("无法连接更新服务，请检查网络后重试")?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        bail!("发布源尚未提供更新清单，可稍后重试或手动下载新版");
    }
    let mut bytes = Vec::new();
    response
        .error_for_status()?
        .take(128 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    let m = verify_manifest(&bytes, crate::PUBLIC_KEY)?;
    Ok((m, bytes))
}

pub(crate) fn newer(version: &str) -> Result<bool> {
    Ok(Version::parse(version)? > Version::parse(CURRENT_VERSION)?)
}
