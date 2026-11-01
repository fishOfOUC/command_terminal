//! Machine-readable contracts. Schema discovery never opens a database.
use crate::{model::*, ApiError, Request, Result};
use schemars::{schema_for, Schema};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

pub(crate) fn id(schema: &mut Schema) {
    schema.insert("minLength".into(), json!(1));
    schema.insert("maxLength".into(), json!(ID_MAX));
    // Unlike $, this end assertion also rejects a final newline.
    schema.insert("pattern".into(), json!(r"^[A-Za-z0-9_-]+(?![\s\S])"));
}

fn text(schema: &mut Schema, max: usize, required: bool) {
    schema.insert("maxLength".into(), json!(max));
    // The explicit whitespace set matches Rust's str::trim (Unicode White_Space).
    let pattern = if required {
        r"^[^\u0000]*[^\u0000\u0009-\u000d\u0020\u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000][^\u0000]*(?![\s\S])"
    } else {
        r"^[^\u0000]*(?![\s\S])"
    };
    schema.insert("pattern".into(), json!(pattern));
    if required {
        schema.insert("minLength".into(), json!(1));
    }
}

pub(crate) fn name(schema: &mut Schema) {
    text(schema, NAME_MAX, true);
}

pub(crate) fn base(schema: &mut Schema) {
    text(schema, VALUE_MAX, true);
}

fn trimmed_text(schema: &mut Schema, max: usize) {
    text(schema, max, true);
    schema.as_object_mut().unwrap().remove("maxLength");
    // Command input is trimmed before validation. Whitespace outside the actual
    // name/base must not count toward its length limit.
    let whitespace =
        r"[\u0009-\u000d\u0020\u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]";
    schema.insert(
        "allOf".into(),
        json!([{
            "pattern":format!(r"^{whitespace}*[\s\S]{{1,{max}}}{whitespace}*(?![\s\S])")
        }]),
    );
}

pub(crate) fn command_name(schema: &mut Schema) {
    trimmed_text(schema, NAME_MAX);
}

pub(crate) fn command_base(schema: &mut Schema) {
    trimmed_text(schema, VALUE_MAX);
}

pub(crate) fn description(schema: &mut Schema) {
    text(schema, DESCRIPTION_MAX, false);
}

pub(crate) fn discovery_request(schema: &mut Schema) {
    schema.insert(
        "not".into(),
        json!({
            "properties":{"list":{"const":true},"operation":{"type":"string"}},
            "required":["list","operation"]
        }),
    );
}

pub(crate) fn parameter(schema: &mut Schema) {
    let value = schema
        .get_mut("properties")
        .unwrap()
        .get_mut("value")
        .unwrap();
    *value = json!({
        "type":["string","number","boolean"],
        "maxLength":VALUE_MAX,
        "pattern":r"^[^\u0000]*(?![\s\S])"
    });
    schema
        .get_mut("required")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(json!("value"));
    schema.insert(
        "allOf".into(),
        json!([{
            "if":{"properties":{"type":{"const":"boolean"}},"required":["type"]},
            "then":{
                "properties":{"value":{"type":"boolean"},"flag":{"minLength":1}},
                "required":["flag"]
            },
            "else":{"properties":{"value":{"type":["string","number"]}}}
        }]),
    );
}

fn conventions() -> Value {
    json!({
        "output":"One JSON envelope on stdout. Success: {schemaVersion:1,ok:true,data:...}. Request/runtime errors: {schemaVersion:1,ok:false,error:{code,message,details}}. command run exit codes 10 and 124 instead return {schemaVersion:1,ok:false,data:...}; inspect data.exitCode, data.timedOut and data.output.",
        "input":"runbook api --input - accepts one request object on stdin. --input FILE reads UTF-8 JSON (UTF-8 BOM allowed); maximum 4 MiB.",
        "updates":"Partial patches; omitted or null fields stay unchanged. params replaces the complete parameter list; [] clears it. IDs, revisions and timestamps are read-only.",
        "concurrency":"Use ifRevision from the last get result. REVISION_CONFLICT means fetch again and reapply your patch.",
        "execution":"These request schemas describe the JSON data API. CRUD/render never executes commands. Execution is CLI-only: runbook command run --project ID --id ID [--values FILE_OR_STDIN] [--if-revision N] [--timeout-ms N]. Timeout defaults to 30000 ms, range 1..86400000.",
        "runtimeValidation":"Schemas describe static structure. Runtime additionally checks directory existence, duplicate IDs/paths/parameter flags, references, revisions, numeric strings and min/max bounds (including min <= max). Required parameter values may be empty when saved, but not when rendered/executed. Command names and base are trimmed before runtime validation.",
        "discovery":"runbook schema --list lists JSON API operations; schema --op OP returns one self-contained request schema. Bare schema preserves the full contract. Schema discovery does not access the database.",
        "exitCodes":{"0":"Success","1":"I/O, database, terminal or execution error","2":"Arguments, JSON or validation error","3":"Not found","4":"Duplicate ID/path or revision conflict","10":"Executed command returned a non-zero exit code; result in data","124":"Execution timed out; result in data"},
        "defaultDataDirectory":"--data-dir overrides RUNBOOK_DATA_DIR, otherwise OS local app data / Runbook."
    })
}

// Retain only definitions reachable from the selected operation, including transitive refs.
// Each emitted requestSchema remains independently usable by JSON Schema validators.
fn collect_definitions(
    value: &Value,
    definitions: &Map<String, Value>,
    used: &mut BTreeSet<String>,
) {
    match value {
        Value::Object(map) => {
            if let Some(name) = map
                .get("$ref")
                .and_then(Value::as_str)
                .and_then(|r| r.strip_prefix("#/$defs/"))
            {
                if used.insert(name.to_owned()) {
                    if let Some(definition) = definitions.get(name) {
                        collect_definitions(definition, definitions, used);
                    }
                }
            }
            for child in map.values() {
                collect_definitions(child, definitions, used);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_definitions(child, definitions, used);
            }
        }
        _ => {}
    }
}

pub fn describe(operation: Option<&str>, list: bool) -> Result<Value> {
    if list && operation.is_some() {
        return Err(ApiError::new(
            "INVALID_ARGUMENT",
            "operation and list cannot be combined",
        ));
    }
    let mut request = serde_json::to_value(schema_for!(Request))?;
    let operations: Vec<_> = request["oneOf"]
        .as_array()
        .expect("Request is a tagged enum")
        .iter()
        .map(|v| {
            v["properties"]["op"]["const"]
                .as_str()
                .expect("operation tag")
                .to_owned()
        })
        .collect();
    if list {
        return Ok(json!({"protocolVersion":1,"operations":operations}));
    }
    // Derive the discovery filter's allowed values from the same operation tags.
    // Null is permitted because this is an optional input field.
    let mut allowed = json!(operations);
    allowed.as_array_mut().unwrap().push(Value::Null);
    let variants = request["oneOf"].as_array_mut().unwrap();
    let discovery = variants
        .iter_mut()
        .find(|v| v["properties"]["op"]["const"] == "schema")
        .unwrap();
    discovery["properties"]["operation"]["enum"] = allowed;
    let variants = request["oneOf"].as_array().unwrap();
    if let Some(operation) = operation {
        let mut selected = variants
            .iter()
            .find(|v| v["properties"]["op"]["const"] == operation)
            .cloned()
            .ok_or_else(|| ApiError {
                code: "INVALID_ARGUMENT".into(),
                message: format!("Unknown JSON API operation '{operation}'. Use schema --list."),
                details: json!({"operation":operation,"operations":operations}),
            })?;
        selected["$schema"] = request["$schema"].clone();
        selected["title"] = json!(operation);
        if let Some(definitions) = request["$defs"].as_object() {
            let mut used = BTreeSet::new();
            collect_definitions(&selected, definitions, &mut used);
            if !used.is_empty() {
                selected["$defs"] = Value::Object(
                    used.into_iter()
                        .map(|name| {
                            let value = definitions[&name].clone();
                            (name, value)
                        })
                        .collect(),
                );
            }
        }
        let mut usage = conventions();
        usage.as_object_mut().unwrap().retain(|key, _| {
            matches!(key.as_str(), "output" | "input" | "runtimeValidation")
                || (key == "updates" && operation.ends_with("_update"))
                || (key == "concurrency"
                    && (operation.ends_with("_update") || operation.ends_with("_delete")))
        });
        return Ok(
            json!({"protocolVersion":1,"operation":operation,"requestSchema":selected,"conventions":usage}),
        );
    }
    // Keep these three standalone schemas for existing consumers. Removing their local
    // definitions would break clients that extract projectSchema or commandSchema.
    Ok(json!({
        "protocolVersion":1,
        "requestSchema":request,
        "projectSchema":schema_for!(Project),
        "commandSchema":schema_for!(Command),
        "conventions":conventions()
    }))
}
