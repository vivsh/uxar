use indexmap::IndexMap;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use super::error::CommandError;

// ── arg types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(super) enum CommandArgType {
    String,
    Number,
    Integer,
    Boolean,
    Array(Box<CommandArgType>),
}

impl CommandArgType {
    pub(super) fn type_name(&self) -> &'static str {
        match self {
            CommandArgType::String => "string",
            CommandArgType::Number => "number",
            CommandArgType::Integer => "integer",
            CommandArgType::Boolean => "boolean",
            CommandArgType::Array(inner) => match **inner {
                CommandArgType::String => "string[]",
                CommandArgType::Number => "number[]",
                CommandArgType::Integer => "integer[]",
                CommandArgType::Boolean => "boolean[]",
                _ => "array",
            },
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CommandArg {
    pub(super) name: String,
    pub(super) arg_type: CommandArgType,
    pub(super) required: bool,
    pub(super) description: Option<String>,
}

// ── schema parsing ────────────────────────────────────────────────────────────

pub(super) fn parse_schema_to_args(
    schema: &schemars::Schema,
) -> Result<Vec<CommandArg>, CommandError> {
    let root_obj = schema
        .as_object()
        .ok_or_else(|| CommandError::Other("Schema is not an object".into()))?;
    let schema_obj = resolve_root_schema_object(root_obj)?;

    let Some(properties) = schema_obj.get("properties").and_then(|p| p.as_object()) else {
        if schema_obj
            .get("type")
            .and_then(|value| value.as_str())
            .is_some_and(|value| value == "object")
        {
            return Ok(Vec::new());
        }
        return Err(CommandError::Other(
            "Command argument schema must be an object with named fields".into(),
        ));
    };

    let required_fields: Vec<String> = schema_obj
        .get("required")
        .and_then(|r| r.as_array())
        .map_or(Vec::new(), |arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        });

    let mut args = Vec::with_capacity(properties.len());
    for (prop, prop_schema) in properties {
        let prop_obj = prop_schema
            .as_object()
            .ok_or_else(|| CommandError::Other("Property schema is not an object".into()))?;

        let arg_type = parse_arg_type(prop_obj)?;
        let required = required_fields.contains(prop);
        let description = prop_obj
            .get("description")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string());

        args.push(CommandArg {
            name: prop.to_string(),
            arg_type,
            required,
            description,
        });
    }
    Ok(args)
}

fn resolve_root_schema_object(
    root_obj: &Map<String, Value>,
) -> Result<Map<String, Value>, CommandError> {
    let Some(reference) = root_obj.get("$ref").and_then(|v| v.as_str()) else {
        return Ok(root_obj.clone());
    };
    let path = reference.strip_prefix("#/").ok_or_else(|| {
        CommandError::Other(format!("Unsupported schema reference: {reference}").into())
    })?;
    let mut value = Value::Object(root_obj.clone());
    for segment in path.split('/') {
        let decoded = segment.replace("~1", "/").replace("~0", "~");
        value = value
            .as_object()
            .and_then(|obj| obj.get(&decoded))
            .cloned()
            .ok_or_else(|| {
                CommandError::Other(format!("Schema reference not found: {reference}").into())
            })?;
    }
    value.as_object().cloned().ok_or_else(|| {
        CommandError::Other(format!("Schema reference is not an object: {reference}").into())
    })
}

fn extract_type_from_schema(schema_obj: &Map<String, Value>) -> Result<&str, CommandError> {
    if let Some(any_of) = schema_obj.get("anyOf") {
        // anyOf: [{type: "X"}, {type: "null"}]  — produced by schemars 0.8.x for Option<T>
        any_of
            .as_array()
            .and_then(|arr| {
                arr.iter().find_map(|v| {
                    v.as_object()
                        .and_then(|o| o.get("type"))
                        .and_then(|t| t.as_str())
                        .filter(|&s| s != "null")
                })
            })
            .ok_or_else(|| CommandError::Other("anyOf schema type not found or unsupported".into()))
    } else if let Some(type_val) = schema_obj.get("type") {
        match type_val {
            // "type": "string"  — scalar, most common case
            Value::String(s) => Ok(s.as_str()),
            // "type": ["string", "null"]  — produced by schemars 1.x for Option<T>
            Value::Array(arr) => arr
                .iter()
                .find_map(|v| v.as_str().filter(|&s| s != "null"))
                .ok_or_else(|| CommandError::Other("type array has no non-null entry".into())),
            _ => Err(CommandError::Other(
                "Property type is not a string or array".into(),
            )),
        }
    } else {
        Err(CommandError::Other("Property type missing".into()))
    }
}

fn parse_array_type(prop_obj: &Map<String, Value>) -> Result<CommandArgType, CommandError> {
    let items_obj = prop_obj
        .get("items")
        .ok_or_else(|| CommandError::Other("Array items schema missing".into()))?
        .as_object()
        .ok_or_else(|| CommandError::Other("Array item schema is not an object".into()))?;
    let item_type_str = extract_type_from_schema(items_obj)?;
    let item_arg_type = match item_type_str {
        "string" => CommandArgType::String,
        "number" => CommandArgType::Number,
        "integer" => CommandArgType::Integer,
        "boolean" => CommandArgType::Boolean,
        _ => return Err(CommandError::UnsupportedType(item_type_str.to_string())),
    };
    Ok(CommandArgType::Array(Box::new(item_arg_type)))
}

fn parse_arg_type(prop_obj: &Map<String, Value>) -> Result<CommandArgType, CommandError> {
    let type_str = extract_type_from_schema(prop_obj)?;
    match type_str {
        "string" => Ok(CommandArgType::String),
        "number" => Ok(CommandArgType::Number),
        "integer" => Ok(CommandArgType::Integer),
        "boolean" => Ok(CommandArgType::Boolean),
        "array" => parse_array_type(prop_obj),
        _ => Err(CommandError::UnsupportedType(type_str.to_string())),
    }
}

// ── cli arg parsing ───────────────────────────────────────────────────────────

pub(super) fn parse_args<T: DeserializeOwned + 'static>(
    command_name: &str,
    args: &[&str],
    arg_defs: &[CommandArg],
) -> Result<T, CommandError> {
    let arg_map: IndexMap<&str, &CommandArg> =
        arg_defs.iter().map(|a| (a.name.as_str(), a)).collect();
    let store = parse_flag_args(command_name, args, &arg_map)?;

    let mut obj = Map::new();
    for (key, values) in &store {
        let arg_def = arg_map
            .get(key.as_str())
            .ok_or_else(|| CommandError::UnknownFlag {
                command: command_name.to_string(),
                flag: key.clone(),
            })?;
        validate_arg_values(key, values, &arg_def.arg_type)?;
        let json_value = convert_value(key, values, &arg_def.arg_type)?;
        obj.insert(key.clone(), json_value);
    }

    check_required_args(arg_defs, &obj)?;

    // Inject false for any boolean flag not supplied on the command line so that
    // serde can deserialize structs whose `bool` fields have no serde default.
    for arg_def in arg_defs {
        if matches!(arg_def.arg_type, CommandArgType::Boolean) && !obj.contains_key(&arg_def.name) {
            obj.insert(arg_def.name.clone(), Value::Bool(false));
        }
    }

    serde_json::from_value(Value::Object(obj))
        .map_err(|e| CommandError::Other(format!("Failed to deserialize arguments: {}", e).into()))
}

fn parse_flag_args(
    command_name: &str,
    args: &[&str],
    arg_map: &IndexMap<&str, &CommandArg>,
) -> Result<IndexMap<String, Vec<String>>, CommandError> {
    let mut store = IndexMap::new();
    let mut current_key: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i];
        if arg.starts_with("--") {
            let (next_i, next_key) = handle_flag(command_name, arg, args, i, &mut store, arg_map)?;
            current_key = next_key;
            i = next_i;
        } else if let Some(key) = current_key.as_deref() {
            store
                .entry(key.to_string())
                .or_default()
                .push(arg.to_string());
            i += 1;
        } else {
            return Err(CommandError::Other(
                format!("Unexpected argument: {}", arg).into(),
            ));
        }
    }
    Ok(store)
}

fn handle_flag(
    command_name: &str,
    flag: &str,
    args: &[&str],
    i: usize,
    store: &mut IndexMap<String, Vec<String>>,
    arg_map: &IndexMap<&str, &CommandArg>,
) -> Result<(usize, Option<String>), CommandError> {
    let key = flag.trim_start_matches("--");

    if let Some(stripped) = key.strip_prefix("no-") {
        if let Some(arg_def) = arg_map.get(stripped) {
            if matches!(arg_def.arg_type, CommandArgType::Boolean) {
                store.insert(stripped.to_string(), vec!["false".to_string()]);
                return Ok((i + 1, None));
            }
        }
        return Err(CommandError::UnknownFlag {
            command: command_name.to_string(),
            flag: key.to_string(),
        });
    }

    if let Some(arg_def) = arg_map.get(key) {
        if matches!(arg_def.arg_type, CommandArgType::Boolean) {
            return handle_bool_flag(key, args, i, store);
        }
    } else {
        return Err(CommandError::UnknownFlag {
            command: command_name.to_string(),
            flag: key.to_string(),
        });
    }

    store.entry(key.to_string()).or_insert_with(Vec::new);
    Ok((i + 1, Some(key.to_string())))
}

fn handle_bool_flag(
    key: &str,
    args: &[&str],
    i: usize,
    store: &mut IndexMap<String, Vec<String>>,
) -> Result<(usize, Option<String>), CommandError> {
    let next_is_bool_value = args
        .get(i + 1)
        .map(|next| !next.starts_with("--") && (*next == "true" || *next == "false"))
        .unwrap_or(false);

    if next_is_bool_value {
        if let Some(next_val) = args.get(i + 1) {
            store.insert(key.to_string(), vec![next_val.to_string()]);
            Ok((i + 2, None))
        } else {
            store.insert(key.to_string(), vec!["true".to_string()]);
            Ok((i + 1, None))
        }
    } else {
        store.insert(key.to_string(), vec!["true".to_string()]);
        Ok((i + 1, None))
    }
}

fn validate_arg_values(
    key: &str,
    values: &[String],
    arg_type: &CommandArgType,
) -> Result<(), CommandError> {
    match arg_type {
        CommandArgType::Array(_) => {
            if values.is_empty() {
                return Err(CommandError::Other(
                    format!("--{} expects at least one value", key).into(),
                ));
            }
        }
        CommandArgType::Boolean => {
            if values.len() != 1 {
                return Err(CommandError::Other(
                    format!("--{} expects exactly one value", key).into(),
                ));
            }
        }
        _ => {
            if values.is_empty() {
                return Err(CommandError::Other(
                    format!("--{} expects exactly one value", key).into(),
                ));
            }
            if values.len() > 1 {
                return Err(CommandError::Other(
                    format!("--{} expects exactly one value, got {}", key, values.len()).into(),
                ));
            }
        }
    }
    Ok(())
}

fn check_required_args(
    arg_defs: &[CommandArg],
    obj: &Map<String, Value>,
) -> Result<(), CommandError> {
    for arg_def in arg_defs {
        // Boolean flags are never "required" from a CLI perspective: absence means false.
        if matches!(arg_def.arg_type, CommandArgType::Boolean) {
            continue;
        }
        if arg_def.required && !obj.contains_key(&arg_def.name) {
            return Err(CommandError::Other(
                format!("Missing required argument: --{}", arg_def.name).into(),
            ));
        }
    }
    Ok(())
}

fn convert_value(
    key: &str,
    values: &[String],
    arg_type: &CommandArgType,
) -> Result<Value, CommandError> {
    match arg_type {
        CommandArgType::Array(item_type) => {
            let mut arr = Vec::with_capacity(values.len());
            for val in values {
                arr.push(convert_single_value(key, val, item_type)?);
            }
            Ok(Value::Array(arr))
        }
        _ => {
            let val = values
                .first()
                .ok_or_else(|| CommandError::Other("No value provided".into()))?;
            convert_single_value(key, val, arg_type)
        }
    }
}

fn convert_single_value(
    key: &str,
    value: &str,
    arg_type: &CommandArgType,
) -> Result<Value, CommandError> {
    match arg_type {
        CommandArgType::String => Ok(Value::String(value.to_string())),
        CommandArgType::Number => {
            let num: f64 =
                value
                    .parse()
                    .map_err(|e: std::num::ParseFloatError| CommandError::ParseError {
                        flag: key.to_string(),
                        value: value.to_string(),
                        expected_type: "number".to_string(),
                        error: e.to_string(),
                    })?;
            Ok(Value::Number(
                serde_json::Number::from_f64(num).ok_or_else(|| CommandError::ParseError {
                    flag: key.to_string(),
                    value: value.to_string(),
                    expected_type: "number".to_string(),
                    error: "invalid floating point value".to_string(),
                })?,
            ))
        }
        CommandArgType::Integer => {
            let num: i64 =
                value
                    .parse()
                    .map_err(|e: std::num::ParseIntError| CommandError::ParseError {
                        flag: key.to_string(),
                        value: value.to_string(),
                        expected_type: "integer".to_string(),
                        error: e.to_string(),
                    })?;
            Ok(Value::Number(serde_json::Number::from(num)))
        }
        CommandArgType::Boolean => {
            let b: bool =
                value
                    .parse()
                    .map_err(|e: std::str::ParseBoolError| CommandError::ParseError {
                        flag: key.to_string(),
                        value: value.to_string(),
                        expected_type: "boolean".to_string(),
                        error: e.to_string(),
                    })?;
            Ok(Value::Bool(b))
        }
        CommandArgType::Array(_) => Err(CommandError::Other(
            "Cannot convert single value to array".into(),
        )),
    }
}
