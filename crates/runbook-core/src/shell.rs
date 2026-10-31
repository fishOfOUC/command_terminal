use crate::{
    model::{validate_value, value_text, Command, ParameterType},
    store::directory,
    ApiError, Result,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub rendered: Option<String>,
}

pub fn quote(value: &str) -> String {
    if cfg!(windows) {
        format!("'{}'", value.replace('\'', "''"))
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
pub fn render(command: &Command, values: &BTreeMap<String, Value>) -> Result<String> {
    for key in values.keys() {
        if !command.params.iter().any(|p| &p.id == key) {
            return Err(ApiError::field(
                "values",
                format!("Unknown parameter ID: {key}"),
            ));
        }
    }
    let mut parts = vec![command.base.trim().to_owned()];
    for p in &command.params {
        let value = values.get(&p.id).unwrap_or(&p.value);
        validate_value(p, value, true)?;
        if p.kind == ParameterType::Boolean {
            if value.as_bool() == Some(true) {
                parts.push(p.flag.clone());
            }
        } else {
            let text = value_text(value);
            if !text.is_empty() {
                if !p.flag.is_empty() {
                    parts.push(p.flag.clone());
                }
                parts.push(quote(&text));
            }
        }
    }
    Ok(parts.join(" "))
}
fn find_program(name: &str) -> Option<String> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|p| p.join(name))
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}
pub fn prepare(path: &str, rendered: Option<String>) -> Result<ShellCommand> {
    let cwd = directory(path)?;
    if cfg!(windows) {
        let program = find_program("pwsh.exe")
            .or_else(|| find_program("powershell.exe"))
            .or_else(|| {
                std::env::var_os("SystemRoot")
                    .map(|r| {
                        PathBuf::from(r).join("System32/WindowsPowerShell/v1.0/powershell.exe")
                    })
                    .filter(|p| p.is_file())
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .ok_or_else(|| ApiError::new("SHELL_NOT_FOUND", "PowerShell was not found"))?;
        let mut args = vec!["-NoLogo".into(), "-NoProfile".into()];
        if let Some(ref line) = rendered {
            // EncodedCommand is UTF-16LE. Literal parameter quoting happens before wrapping.
            let script=format!("[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); $OutputEncoding = [Console]::OutputEncoding; $ErrorActionPreference = 'Stop'; $global:LASTEXITCODE = 0; try {{ & {{\n{line}\n}}; if (-not $?) {{ exit 1 }}; exit $global:LASTEXITCODE }} catch {{ [Console]::Error.WriteLine($_.ToString()); exit 1 }}");
            let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
            args.extend(["-EncodedCommand".into(), STANDARD.encode(bytes)]);
        }
        Ok(ShellCommand {
            program,
            args,
            cwd,
            rendered,
        })
    } else {
        let program = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let args = rendered
            .as_ref()
            .map(|s| vec!["-c".into(), s.clone()])
            .unwrap_or_default();
        Ok(ShellCommand {
            program,
            args,
            cwd,
            rendered,
        })
    }
}
