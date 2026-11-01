use crate::{ApiError, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

pub(crate) const ID_MAX: usize = 100;
pub(crate) const NAME_MAX: usize = 60;
pub(crate) const DESCRIPTION_MAX: usize = 1000;
pub(crate) const VALUE_MAX: usize = 16384;
pub(crate) const MAX_PARAMETERS: usize = 64;

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub fn new_id(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::new_v4())
}
pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > ID_MAX
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(ApiError::field(
            "id",
            "IDs must contain 1..100 ASCII letters, digits, hyphens or underscores",
        ));
    }
    Ok(())
}
pub fn text(value: &str, field: &str, max: usize, required: bool) -> Result<()> {
    if (required && value.trim().is_empty()) || value.chars().count() > max || value.contains('\0')
    {
        return Err(ApiError::field(
            field,
            format!(
                "{field} must contain {}..{max} characters without NUL",
                usize::from(required)
            ),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(transform = crate::schema::parameter)]
pub struct Parameter {
    #[schemars(transform = crate::schema::id)]
    pub id: String,
    #[schemars(transform = crate::schema::name)]
    pub label: String,
    #[serde(default)]
    #[schemars(regex(pattern = r"^(?:|-{1,2}[A-Za-z][A-Za-z0-9_-]*)(?![\s\S])"))]
    pub flag: String,
    #[serde(rename = "type")]
    pub kind: ParameterType,
    #[serde(default)]
    pub value: Value,
    #[serde(default)]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[schemars(inline)]
pub enum ParameterType {
    Text,
    Number,
    Boolean,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Command {
    #[schemars(transform = crate::schema::id)]
    pub id: String,
    #[schemars(transform = crate::schema::name)]
    pub name: String,
    #[schemars(transform = crate::schema::base)]
    pub base: String,
    #[serde(default)]
    #[schemars(transform = crate::schema::description)]
    pub description: String,
    #[serde(default)]
    #[schemars(length(max = MAX_PARAMETERS))]
    pub params: Vec<Parameter>,
    pub revision: u64,
    pub created_at: u64,
    pub updated_at: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Project {
    #[schemars(transform = crate::schema::id)]
    pub id: String,
    #[schemars(transform = crate::schema::name)]
    pub name: String,
    pub path: String,
    #[serde(default)]
    #[schemars(transform = crate::schema::description)]
    pub description: String,
    #[serde(default)]
    pub starred: bool,
    #[serde(default)]
    pub archived: bool,
    pub opened_at: Option<u64>,
    pub created_at: u64,
    pub updated_at: u64,
    pub revision: u64,
    pub commands: Vec<Command>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectCreate {
    #[schemars(transform = crate::schema::id)]
    pub id: Option<String>,
    #[schemars(transform = crate::schema::name)]
    pub name: String,
    pub path: String,
    #[serde(default)]
    #[schemars(transform = crate::schema::description)]
    pub description: String,
    #[serde(default)]
    pub starred: bool,
}
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectPatch {
    #[schemars(transform = crate::schema::name)]
    pub name: Option<String>,
    pub path: Option<String>,
    #[schemars(transform = crate::schema::description)]
    pub description: Option<String>,
    pub starred: Option<bool>,
    pub archived: Option<bool>,
}
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandCreate {
    #[schemars(transform = crate::schema::id)]
    pub id: Option<String>,
    #[schemars(transform = crate::schema::command_name)]
    pub name: String,
    #[schemars(transform = crate::schema::command_base)]
    pub base: String,
    #[serde(default)]
    #[schemars(transform = crate::schema::description)]
    pub description: String,
    #[serde(default)]
    #[schemars(length(max = MAX_PARAMETERS))]
    pub params: Vec<Parameter>,
}
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandPatch {
    #[schemars(transform = crate::schema::command_name)]
    pub name: Option<String>,
    #[schemars(transform = crate::schema::command_base)]
    pub base: Option<String>,
    #[schemars(transform = crate::schema::description)]
    pub description: Option<String>,
    #[schemars(length(max = MAX_PARAMETERS))]
    pub params: Option<Vec<Parameter>>,
}

pub fn validate_command(command: &Command) -> Result<()> {
    validate_id(&command.id)?;
    text(&command.name, "name", NAME_MAX, true)?;
    text(&command.base, "base", VALUE_MAX, true)?;
    text(&command.description, "description", DESCRIPTION_MAX, false)?;
    if command.params.len() > MAX_PARAMETERS {
        return Err(ApiError::field(
            "params",
            "At most 64 parameters are supported",
        ));
    }
    let mut ids = HashSet::new();
    let mut flags = HashSet::new();
    for p in &command.params {
        validate_id(&p.id)?;
        if !ids.insert(&p.id) {
            return Err(ApiError::field("params.id", "Duplicate parameter ID"));
        }
        text(&p.label, "params.label", NAME_MAX, true)?;
        if !p.flag.is_empty() {
            let rest = p.flag.trim_start_matches('-');
            let prefix = p.flag.len() - rest.len();
            if !(1..=2).contains(&prefix)
                || rest.is_empty()
                || !rest.as_bytes()[0].is_ascii_alphabetic()
                || !rest
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            {
                return Err(ApiError::field(
                    "params.flag",
                    "Flags must be like --port or -p; leave blank for positional arguments",
                ));
            }
            if !flags.insert(&p.flag) {
                return Err(ApiError::field("params.flag", "Duplicate parameter flag"));
            }
        }
        if p.kind == ParameterType::Boolean && (p.flag.is_empty() || !p.value.is_boolean()) {
            return Err(ApiError::field(
                "params.value",
                "Boolean parameters require a flag and a boolean value",
            ));
        }
        if p.kind != ParameterType::Boolean && !p.value.is_string() && !p.value.is_number() {
            return Err(ApiError::field(
                "params.value",
                "Text/number parameter values must be strings or numbers",
            ));
        }
        if p.min.zip(p.max).is_some_and(|(a, b)| a > b) {
            return Err(ApiError::field("params.min", "min must not exceed max"));
        }
        validate_value(p, &p.value, false)?;
    }
    Ok(())
}
pub fn value_text(v: &Value) -> String {
    v.as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| v.to_string())
}
pub fn validate_value(p: &Parameter, value: &Value, executing: bool) -> Result<()> {
    let err = |message: &str| ApiError {
        code: "VALIDATION_ERROR".into(),
        message: message.into(),
        details: json!({"parameterId":p.id}),
    };
    if p.kind == ParameterType::Boolean {
        if !value.is_boolean() {
            return Err(err("Boolean parameter requires true or false"));
        }
        return Ok(());
    }
    if !value.is_string() && !value.is_number() {
        return Err(err("Parameter must be a string or number"));
    }
    let s = value_text(value);
    text(&s, "parameter.value", VALUE_MAX, false)?;
    if s.trim().is_empty() {
        return if executing && p.required {
            Err(err("Required parameter is empty"))
        } else {
            Ok(())
        };
    }
    if p.kind == ParameterType::Number {
        let n = s
            .parse::<f64>()
            .map_err(|_| err("Number parameter is not a number"))?;
        if !n.is_finite() || p.min.is_some_and(|min| n < min) || p.max.is_some_and(|max| n > max) {
            return Err(err("Number parameter is outside its allowed range"));
        }
    }
    Ok(())
}
