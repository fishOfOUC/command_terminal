use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path},
};

pub(crate) const REQUIRED: [&str; 3] =
    ["Runbook-Desktop.exe", "runbook.exe", "runbook-updater.exe"];

// Keep this allowlist deliberately narrower than arbitrary zip extraction.
pub(crate) fn safe_name(name: &str) -> bool {
    if name.is_empty() || name.contains(['\\', ':', '\0']) {
        return false;
    }
    let path = Path::new(name);
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return false;
    }
    if name.split('/').any(|s| {
        s.is_empty()
            || s == "."
            || s == ".."
            || s.ends_with(['.', ' '])
            || matches!(
                s.split('.')
                    .next()
                    .unwrap_or("")
                    .to_ascii_uppercase()
                    .as_str(),
                "CON"
                    | "PRN"
                    | "AUX"
                    | "NUL"
                    | "COM1"
                    | "COM2"
                    | "COM3"
                    | "COM4"
                    | "COM5"
                    | "COM6"
                    | "COM7"
                    | "COM8"
                    | "COM9"
                    | "LPT1"
                    | "LPT2"
                    | "LPT3"
                    | "LPT4"
                    | "LPT5"
                    | "LPT6"
                    | "LPT7"
                    | "LPT8"
                    | "LPT9"
            )
            || !s
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._- ".contains(&c))
    }) {
        return false;
    }
    REQUIRED.contains(&name)
        || [
            "README.md",
            "LICENSE",
            "AI_GUIDE.md",
            "docs",
            "scripts",
            "skills",
        ]
        .contains(&name)
        || ["docs/", "scripts/", "skills/"]
            .iter()
            .any(|p| name.starts_with(p))
}

pub(crate) fn no_links(root: &Path, relative: &str) -> Result<()> {
    let mut path = root.to_path_buf();
    for part in Path::new(relative).components() {
        path.push(part);
        if let Ok(meta) = fs::symlink_metadata(&path) {
            ensure!(!meta.file_type().is_symlink(), "更新目标包含目录链接");
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(meta.file_attributes() & 0x400 == 0, "更新目标包含重解析点");
            }
        }
    }
    Ok(())
}

pub(crate) fn verify_archive(path: &Path, manifest: &crate::Manifest) -> Result<()> {
    let mut input = File::open(path)?;
    ensure!(
        input.metadata()?.len() == manifest.size,
        "更新包不完整，请重新下载"
    );
    let mut hash = Sha256::new();
    let mut chunk = [0; 64 * 1024];
    loop {
        let n = input.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        hash.update(&chunk[..n]);
    }
    ensure!(
        format!("{:x}", hash.finalize()).eq_ignore_ascii_case(&manifest.sha256),
        "更新包校验失败，请重新下载"
    );
    Ok(())
}

pub(crate) fn extract(archive: &Path, output: &Path) -> Result<Vec<String>> {
    let mut zip = zip::ZipArchive::new(File::open(archive)?).context("更新包不是有效 ZIP")?;
    ensure!(zip.len() <= 1000, "更新包文件数量过多");
    fs::create_dir_all(output)?;
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let name = entry.name().to_owned();
        let clean = name.trim_end_matches('/');
        ensure!(
            entry.enclosed_name().is_some() && safe_name(clean),
            "更新包包含未授权路径: {name}"
        );
        ensure!(
            entry
                .unix_mode()
                .map(|m| m & 0o170000 != 0o120000)
                .unwrap_or(true),
            "更新包包含符号链接"
        );
        ensure!(
            seen.insert(clean.to_ascii_lowercase()),
            "更新包包含重复路径"
        );
        total = total.checked_add(entry.size()).context("更新包大小溢出")?;
        ensure!(total <= 512 * 1024 * 1024, "解压后的更新包过大");
        if entry.is_dir() {
            continue;
        }
        ensure!(
            !["docs", "scripts", "skills"].contains(&name.as_str()),
            "目录名称不能用作文件"
        );
        let target = output.join(&name);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = File::create(&target)?;
        let copied = std::io::copy(&mut entry, &mut file)?;
        ensure!(copied == entry.size(), "更新文件不完整");
        file.flush()?;
        file.sync_all()?;
        names.push(name);
    }
    ensure!(
        REQUIRED.iter().all(|n| names.iter().any(|s| s == n)),
        "更新包缺少桌面程序、CLI 或更新器"
    );
    Ok(names)
}

/// Replace a file without exposing a missing executable between rename steps.
pub(crate) fn replace(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination.parent().context("更新目标没有父目录")?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    std::io::copy(&mut File::open(source)?, &mut temp)?;
    temp.as_file().sync_all()?;
    temp.persist(destination)
        .map_err(|e| e.error)
        .context("文件被占用或无法替换")?;
    Ok(())
}
