use std::{any::TypeId, sync::Arc};
use indexmap::IndexMap;
use serde::{de::DeserializeOwned};
use serde_json::{Map, Value};

use crate::{Site, callables::{self, Callable}};


type SchemaGen = fn(&mut schemars::SchemaGenerator) -> schemars::Schema;


pub type CommandHandlerIn = Callable<CommandContext, CommandError>;

pub struct CommandConf{
    pub name: Option<String>
}

pub struct CommandContext{
    site: Site,
    payload: callables::PayloadData,  
}

impl callables::IntoPayloadData for CommandContext {
 
    fn into_payload_data(self) -> callables::PayloadData {
        self.payload
    }

}

pub(crate) struct Command{
    pub(crate) handler: CommandHandlerIn,
    pub(crate) options: CommandConf,
    pub(crate) args: Vec<CommandArg>,
    pub(crate) parser: fn(&[&str], &[CommandArg]) -> Result<callables::PayloadData, CommandError>,
}


pub(crate) fn command<T, H, Args>(handler: H, options: CommandConf) -> Command
where
    T: callables::Payloadable,
    H: callables::Specable<Args, Output = Result<(), CommandError>> + Send + Sync + 'static,
    Args: callables::FromContext<CommandContext> + callables::IntoArgSpecs + callables::HasPayload<T> + Send + 'static,
{
    let mut callable: Callable<CommandContext, CommandError> = Callable::new(handler);

    let schema = schemars::schema_for!(T);

    let args = CommandRegistry::parse_schema_to_arg_types(&schema).unwrap();

    let parser: fn(&[&str], &[CommandArg]) -> Result<callables::PayloadData, CommandError> = |cli: &[&str], args: &[CommandArg]|{
        let obj: T = CommandRegistry::parse_args(cli, args)?;
        return Ok(callables::PayloadData::new(obj));
    };

    // Override type_id with explicit payload type for dispatch routing
    callable.type_id = TypeId::of::<T>();
    Command {
        handler: callable,
        options,
        args,
        parser,
    }
}



#[derive(Debug, Clone)]
enum CommandArgType {
    String,
    Number,
    Integer,
    Boolean,
    Array(Box<CommandArgType>),
}

impl CommandArgType {

    fn type_name(&self) -> &'static str {
        let type_name = match self {
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
        };
        type_name
    }

}

#[derive(Debug, Clone)]
struct CommandArg {
    name: String,
    arg_type: CommandArgType,
    required: bool,
    description: Option<String>,
}


#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// argh uses EarlyExit for both `--help` and parse errors.
    /// Treat it specially in `execute()`; do NOT rely on Display/Debug for UX.
    #[error("Argument parsing exit")]
    Exit(argh::EarlyExit),

    #[error("Command not found: {0}")]
    NotFound(String),

    #[error("Command already exists: {0}")]
    AlreadyExists(String),

    #[error("Unsupported type for command argument: {0}")]
    UnsupportedType(String),

    #[error("Failed to parse {value} as {expected_type}: {error}")]
    ParseError {
        value: String,
        expected_type: String,
        error: String,
    },

    #[error(transparent)]
    CallError(#[from] callables::CallError),

    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// A registry for CLI commands
/// S is the state type passed to command handlers. It can be used to pass context or site to the handler. 
/// It can be used to register commands, generate help messages, and execute commands
pub struct CommandRegistry {
    banner: Option<String>,
    commands: IndexMap<String, Command>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            banner: None,
            commands: IndexMap::new(),
        }
    }

    pub fn with_banner(mut self, banner: String) -> Self {
        self.banner = Some(banner);
        self
    }

    pub fn merge(&mut self, other: CommandRegistry) -> Result<(), CommandError> {
        for (name, handler) in other.commands {
            if self.commands.contains_key(&name) {
                return Err(CommandError::AlreadyExists(name));
            }
            self.commands.insert(name, handler);
        }
        Ok(())
    }

    fn convert_value(values: &[String], arg_type: &CommandArgType) -> Result<Value, CommandError> {
        match arg_type {
            CommandArgType::Array(item_type) => {
                let mut arr = Vec::with_capacity(values.len());
                for val in values {
                    let converted = Self::convert_single_value(val, item_type)?;
                    arr.push(converted);
                }
                Ok(Value::Array(arr))
            }
            _ => {
                // Handle single value (take first element)
                let val = values
                    .first()
                    .ok_or_else(|| CommandError::Other("No value provided".into()))?;
                Self::convert_single_value(val, arg_type)
            }
        }
    }

    fn convert_single_value(value: &str, arg_type: &CommandArgType) -> Result<Value, CommandError> {
        match arg_type {
            CommandArgType::String => Ok(Value::String(value.to_string())),
            CommandArgType::Number => {
                let num: f64 = value.parse().map_err(|e: std::num::ParseFloatError| {
                    CommandError::ParseError {
                        value: value.to_string(),
                        expected_type: "number".to_string(),
                        error: e.to_string(),
                    }
                })?;
                Ok(Value::Number(
                    serde_json::Number::from_f64(num).ok_or_else(|| CommandError::ParseError {
                        value: value.to_string(),
                        expected_type: "number".to_string(),
                        error: "invalid floating point value".to_string(),
                    })?,
                ))
            }
            CommandArgType::Integer => {
                let num: i64 = value.parse().map_err(|e: std::num::ParseIntError| {
                    CommandError::ParseError {
                        value: value.to_string(),
                        expected_type: "integer".to_string(),
                        error: e.to_string(),
                    }
                })?;
                Ok(Value::Number(serde_json::Number::from(num)))
            }
            CommandArgType::Boolean => {
                let b: bool = value.parse().map_err(|e: std::str::ParseBoolError| {
                    CommandError::ParseError {
                        value: value.to_string(),
                        expected_type: "boolean".to_string(),
                        error: e.to_string(),
                    }
                })?;
                Ok(Value::Bool(b))
            }
            CommandArgType::Array(_) => Err(CommandError::Other(
                "Cannot convert single value to array".into(),
            )),
        }
    }

    fn extract_type_from_schema(schema_obj: &Map<String, Value>) -> Result<&str, CommandError> {
        // Handle anyOf for Option<T> types - schemars generates {"anyOf": [{"type": "..."}, {"type": "null"}]}
        if let Some(any_of) = schema_obj.get("anyOf") {
            // For Option<T>, find the non-null type
            any_of
                .as_array()
                .and_then(|arr| {
                    arr.iter()
                        .find_map(|v| {
                            v.as_object()
                                .and_then(|o| o.get("type"))
                                .and_then(|t| t.as_str())
                                .filter(|&s| s != "null")
                        })
                })
                .ok_or_else(|| {
                    CommandError::Other("anyOf schema type not found or unsupported".into())
                })
        } else {
            schema_obj
                .get("type")
                .and_then(|t| t.as_str())
                .ok_or_else(|| {
                    CommandError::Other("Property type missing or not a string".into())
                })
        }
    }

    fn parse_array_type(prop_obj: &Map<String, Value>) -> Result<CommandArgType, CommandError> {
        let items_schema = prop_obj
            .get("items")
            .ok_or_else(|| CommandError::Other("Array items schema missing".into()))?;
        let items_obj = items_schema.as_object().ok_or_else(|| {
            CommandError::Other("Array item schema is not an object".into())
        })?;
        let item_type_str = Self::extract_type_from_schema(items_obj)?;
        let item_arg_type = match item_type_str {
            "string" => CommandArgType::String,
            "number" => CommandArgType::Number,
            "integer" => CommandArgType::Integer,
            "boolean" => CommandArgType::Boolean,
            _ => return Err(CommandError::UnsupportedType(item_type_str.to_string())),
        };
        Ok(CommandArgType::Array(Box::new(item_arg_type)))
    }

    fn parse_arg_type(
        prop_obj: &Map<String, Value>,
    ) -> Result<CommandArgType, CommandError> {
        let type_str = Self::extract_type_from_schema(prop_obj)?;
        
        match type_str {
            "string" => Ok(CommandArgType::String),
            "number" => Ok(CommandArgType::Number),
            "integer" => Ok(CommandArgType::Integer),
            "boolean" => Ok(CommandArgType::Boolean),
            "array" => Self::parse_array_type(prop_obj),
            _ => Err(CommandError::UnsupportedType(type_str.to_string())),
        }
    }

    fn parse_schema_to_arg_types(
        schema: &schemars::Schema,
    ) -> Result<Vec<CommandArg>, CommandError> {
        let schema_obj = schema
            .as_object()
            .ok_or_else(|| CommandError::Other("Schema is not an object".into()))?;

        let properties = schema_obj
            .get("properties")
            .and_then(|p| p.as_object())
            .ok_or_else(|| {
                CommandError::Other("Schema properties missing or not an object".into())
            })?;

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
            
            let arg_type = Self::parse_arg_type(prop_obj)?;
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

    fn parse_flag_args(
        args: &[&str],
        arg_map: &IndexMap<&str, &CommandArg>,
    ) -> Result<IndexMap<String, Vec<String>>, CommandError> {
        let mut store = IndexMap::new();
        let mut i = 0;
        
        while i < args.len() {
            let arg = args[i];
            if arg.starts_with("--") {
                i = Self::handle_flag(arg, args, i, &mut store, arg_map)?;
            } else if let Some((_last_key, values)) = store.last_mut() {
                values.push(arg.to_string());
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
        flag: &str,
        args: &[&str],
        i: usize,
        store: &mut IndexMap<String, Vec<String>>,
        arg_map: &IndexMap<&str, &CommandArg>,
    ) -> Result<usize, CommandError> {
        let key = flag.trim_start_matches("--");
        
        // Handle --no-<flag> for booleans
        if let Some(stripped) = key.strip_prefix("no-") {
            if let Some(arg_def) = arg_map.get(stripped) {
                if matches!(arg_def.arg_type, CommandArgType::Boolean) {
                    store.insert(stripped.to_string(), vec!["false".to_string()]);
                    return Ok(i + 1);
                }
            }
        }
        
        // Check if this is a boolean flag
        if let Some(arg_def) = arg_map.get(key) {
            if matches!(arg_def.arg_type, CommandArgType::Boolean) {
                return Self::handle_bool_flag(key, args, i, store);
            }
        }
        
        // Non-boolean flag, expect values to follow
        store.entry(key.to_string()).or_insert_with(Vec::new);
        Ok(i + 1)
    }

    fn handle_bool_flag(
        key: &str,
        args: &[&str],
        i: usize,
        store: &mut IndexMap<String, Vec<String>>,
    ) -> Result<usize, CommandError> {
        // Check if next arg is a boolean value or another flag
        let next_is_bool_value = args.get(i + 1)
            .map(|next| !next.starts_with("--") && (*next == "true" || *next == "false"))
            .unwrap_or(false);
        
        if next_is_bool_value {
            // Explicit boolean value provided - safe because we checked above
            if let Some(next_val) = args.get(i + 1) {
                store.insert(key.to_string(), vec![next_val.to_string()]);
                Ok(i + 2)
            } else {
                // Should never happen due to check above
                store.insert(key.to_string(), vec!["true".to_string()]);
                Ok(i + 1)
            }
        } else {
            // Flag without value means true
            store.insert(key.to_string(), vec!["true".to_string()]);
            Ok(i + 1)
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
            if arg_def.required && !obj.contains_key(&arg_def.name) {
                return Err(CommandError::Other(
                    format!("Missing required argument: --{}", arg_def.name).into(),
                ));
            }
        }
        Ok(())
    }

    /// convert args to serde_json::Value based on pre-computed arg types
    /// no positional or short arguments supported
    /// no nested structures supported
    /// only string, number, boolean types, and arrays of these scalars are supported
    /// e.g. --name value --age 30 --tags tag1 --tags tag2
    /// boolean flags: --verbose (true), --no-verbose (false), --verbose true (explicit)
    fn parse_args<T: DeserializeOwned + 'static>(
        args: &[&str],
        arg_defs: &[CommandArg],
    ) -> Result<T, CommandError> {
        // Create a map for quick lookup of arg types
        let arg_map: IndexMap<&str, &CommandArg> = arg_defs
            .iter()
            .map(|arg| (arg.name.as_str(), arg))
            .collect();

        // Parse flags and values
        let store = Self::parse_flag_args(args, &arg_map)?;

        // Convert to serde_json::Value
        let mut obj = Map::new();
        for (key, values) in &store {
            let arg_def = arg_map
                .get(key.as_str())
                .ok_or_else(|| CommandError::Other(format!("Unknown property: {}", key).into()))?;
            
            Self::validate_arg_values(key, values, &arg_def.arg_type)?;
            let json_value = Self::convert_value(values, &arg_def.arg_type)?;
            obj.insert(key.clone(), json_value);
        }
        
        Self::check_required_args(arg_defs, &obj)?;

        // convert to T
        serde_json::from_value(Value::Object(obj)).map_err(|e| {
            CommandError::Other(format!("Failed to deserialize arguments: {}", e).into())
        })
    }

    pub fn iter_commands(&self) -> impl Iterator<Item = &Command> {
        self.commands.values()
    }

    pub(crate) fn register(&mut self, command: Command) -> Result<(), CommandError>
    {
        let command_name = command.options.name.as_ref().map(|s| s.as_str()).unwrap_or_else(|| command.handler.inspect().name.as_str());
        if command_name == "help" || self.commands.contains_key(command_name) {
            return Err(CommandError::AlreadyExists(command_name.to_string()));
        }

        self.commands.insert(
            command_name.to_string(),
            command,
        );
        Ok(())
    }

    pub fn generate_help(&self, command_name: &str) -> Result<String, CommandError> {
        let command = self
            .commands
            .get(command_name)
            .ok_or_else(|| CommandError::NotFound(command_name.to_string()))?;

        let mut help_msg = format!("Usage: {} [OPTIONS]\n\nOptions:\n", command_name);
        for arg in &command.args {
            let required_str = if arg.required { " (required)" } else { "" };

            let desc_str = arg
                .description
                .as_ref()
                .map(|d| format!(" - {}", d))
                .unwrap_or_default();

            let help_line = if matches!(arg.arg_type, CommandArgType::Boolean) {
                format!(
                    "  --{} / --no-{}{}{}\n",
                    arg.name, arg.name, required_str, desc_str
                )
            } else {
                let type_name = &arg.arg_type.type_name();
                format!(
                    "  --{} <{}>{}{}\n",
                    arg.name, type_name, required_str, desc_str
                )
            };
            help_msg.push_str(&help_line);
        }
        Ok(help_msg)
    }

    pub async fn execute(&self, command_name: &str, args: &[&str], site: Site) -> Result<(), CommandError> {
        if command_name == "help" {
            let help_msg = self.execute_help();
            println!("{}", help_msg);
            return Ok(());
        }
        
        // Check if --help flag is present
        if args.contains(&"--help") {
            let help_msg = self.generate_help(command_name)?;
            println!("{}", help_msg);
            return Ok(());
        }
        
        let command = self
            .commands
            .get(command_name)
            .ok_or_else(|| CommandError::NotFound(command_name.to_string()))?;

        let payload = (command.parser)(args, &command.args)?;
        let ctx = CommandContext {
            site,
            payload,
        };

        match command.handler.call(ctx).await{
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
        
    }

    fn execute_help(&self) -> String {
        let mut help = String::new();
        
        if let Some(banner) = &self.banner {
            help.push_str(banner);
            help.push_str("\n\n");
        }
        
        help.push_str("Available commands:\n\n");
        
        for (name, command) in &self.commands {
            let description = &command.handler.inspect().description;
            let summary = if let Some(desc) = description.as_ref() {
                // Get first line of description
                desc.lines().next().unwrap_or("")
            } else {
                "No description available"
            };
            help.push_str(&format!("  {:<20} {}", name, summary));
            help.push('\n');
        }
        
        help.push_str("\nUse '<command> --help' for more information on a specific command.\n");
        help
    }
}


mod tests{
    use super::*;
    use crate::{Site, SiteConf, build_site};
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    async fn make_site() -> Site {
        let conf = SiteConf::default();
        let bundle = crate::bundles::Bundle::new();
        let site = build_site(conf, bundle).await.unwrap();
        site
    }

    #[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
    struct TestArgs {
        name: String,
        age: i32,
        #[serde(default)]
        verbose: bool,
    }

    #[tokio::test]
    async fn test_execute_command() {
        async fn handler(args: callables::Payload<TestArgs>) -> Result<(), CommandError> {
            assert_eq!(args.name, "Alice");
            assert_eq!(args.age, 30);
            assert!(args.verbose);
            Ok(())
        }

        let mut registry = CommandRegistry::new();
        
        let cmd = command::<TestArgs, _, _>(
            handler,
            CommandConf { name: Some("test".to_string()) },
        );
        
        registry.register(cmd).unwrap();
        
        // Execute the command with valid args
        let site = make_site().await;
        let result = registry.execute(
            "test",
            &["--name", "Alice", "--age", "30", "--verbose"],
            site,
        ).await;
        
        assert!(result.is_ok());
    }

}