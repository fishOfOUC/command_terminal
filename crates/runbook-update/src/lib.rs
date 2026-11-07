//! Signed portable updates. The application and CLI share a runtime lock;
//! the separate updater owns file replacement and crash recovery.
mod controller;
mod install;
mod manifest;
mod package;

pub use controller::{Controller, UpdateStatus};
pub use install::{acknowledge, is_recovery_scheduled, run_plan, run_recovery, RuntimeGuard};
pub use manifest::{verify_manifest, Manifest, CURRENT_VERSION};

pub const PUBLIC_KEY: &str = include_str!("../../../assets/update-public-key.txt");

#[cfg(test)]
mod tests;

pub fn application_dir() -> anyhow::Result<std::path::PathBuf> {
    let exe = std::env::current_exe()?;
    dunce::canonicalize(
        exe.parent()
            .ok_or_else(|| anyhow::anyhow!("程序目录不可用"))?,
    )
    .map_err(Into::into)
}
