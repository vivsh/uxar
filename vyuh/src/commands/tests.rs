use crate::{Site, SiteConf, build_site};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::args::{parse_args, parse_schema_to_args};
use super::error::CommandError;
use super::registry::CommandRegistry;
use super::types::{CommandArgs, CommandConf, command};

async fn make_site() -> Site {
    let conf = SiteConf::default().log_init(false);
    let bundle = crate::bundles::Bundle::new();
    build_site(conf, bundle).await.unwrap()
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct TestArgs {
    name: String,
    age: i32,
    #[serde(default)]
    verbose: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq)]
struct ParserArgs {
    name: String,
    age: i32,
    #[serde(default)]
    verbose: bool,
    tags: Vec<String>,
}

fn command_arg_defs<T: JsonSchema>() -> Vec<super::args::CommandArg> {
    let mut settings = schemars::generate::SchemaSettings::default();
    settings.inline_subschemas = true;
    let mut generator = schemars::SchemaGenerator::new(settings);
    let schema = generator.subschema_for::<T>();
    parse_schema_to_args(&schema).unwrap()
}

fn test_command() -> super::Command {
    async fn handler(_args: CommandArgs<TestArgs>) -> Result<(), CommandError> {
        Ok(())
    }

    command::<TestArgs, _, _>(handler, CommandConf::new("test")).unwrap()
}

#[tokio::test]
async fn test_execute_command() {
    async fn handler(args: CommandArgs<TestArgs>) -> Result<(), CommandError> {
        assert_eq!(args.name, "Alice");
        assert_eq!(args.age, 30);
        assert!(args.verbose);
        Ok(())
    }

    let mut registry = CommandRegistry::new();
    let cmd = command::<TestArgs, _, _>(handler, CommandConf::new("test")).unwrap();
    registry.register(cmd).unwrap();

    let site = make_site().await;
    let result = registry
        .execute(
            "test",
            &["--name", "Alice", "--age", "30", "--verbose"],
            site,
        )
        .await;
    assert!(result.is_ok());
}

#[test]
fn test_parse_arrays_booleans_and_scalars() {
    let args = command_arg_defs::<ParserArgs>();
    let parsed = parse_args::<ParserArgs>(
        "parse",
        &[
            "--name",
            "Alice",
            "--age",
            "30",
            "--tags",
            "api",
            "web",
            "--verbose",
            "false",
            "--tags",
            "admin",
        ],
        &args,
    )
    .unwrap();

    assert_eq!(
        parsed,
        ParserArgs {
            name: "Alice".to_string(),
            age: 30,
            verbose: false,
            tags: vec!["api".to_string(), "web".to_string(), "admin".to_string()],
        }
    );
}

#[test]
fn test_parse_no_bool_flag() {
    let args = command_arg_defs::<ParserArgs>();
    let parsed = parse_args::<ParserArgs>(
        "parse",
        &[
            "--name",
            "Alice",
            "--age",
            "30",
            "--tags",
            "api",
            "--no-verbose",
        ],
        &args,
    )
    .unwrap();

    assert!(!parsed.verbose);
}

#[test]
fn test_parse_errors_are_specific() {
    let args = command_arg_defs::<ParserArgs>();

    let err = parse_args::<ParserArgs>("parse", &["--unknown"], &args).unwrap_err();
    assert!(matches!(
        err,
        CommandError::UnknownFlag {
            command,
            flag
        } if command == "parse" && flag == "unknown"
    ));

    let err = parse_args::<ParserArgs>("parse", &["--name", "Alice", "--tags", "api"], &args)
        .unwrap_err();
    assert!(err.to_string().contains("Missing required argument: --age"));

    let err = parse_args::<ParserArgs>(
        "parse",
        &["--name", "Alice", "--age", "not-int", "--tags", "api"],
        &args,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        CommandError::ParseError {
            flag,
            expected_type,
            ..
        } if flag == "age" && expected_type == "integer"
    ));
}

#[tokio::test]
async fn test_duplicate_and_reserved_command_names_fail() {
    let mut registry = CommandRegistry::new();
    registry.register(test_command()).unwrap();

    let duplicate = registry.register(test_command()).unwrap_err();
    assert!(matches!(duplicate, CommandError::AlreadyExists(name) if name == "test"));

    async fn help_handler(_args: CommandArgs<TestArgs>) -> Result<(), CommandError> {
        Ok(())
    }

    let reserved = command::<TestArgs, _, _>(help_handler, CommandConf::new("help")).unwrap();
    let err = CommandRegistry::new().register(reserved).unwrap_err();
    assert!(matches!(err, CommandError::AlreadyExists(name) if name == "help"));
}

#[tokio::test]
async fn test_unknown_command_mentions_help() {
    let registry = CommandRegistry::new();
    let site = make_site().await;
    let err = registry.execute("missing", &[], site).await.unwrap_err();
    assert!(matches!(err, CommandError::UnknownCommand(ref name) if name == "missing"));
    assert!(err.to_string().contains("help"));
}

#[test]
fn test_help_is_sorted_and_uses_descriptions() {
    async fn alpha(_args: CommandArgs<TestArgs>) -> Result<(), CommandError> {
        Ok(())
    }

    async fn beta(_args: CommandArgs<TestArgs>) -> Result<(), CommandError> {
        Ok(())
    }

    let mut registry = CommandRegistry::new();
    registry
        .register(
            command::<TestArgs, _, _>(beta, CommandConf::new("beta").description("Beta command."))
                .unwrap(),
        )
        .unwrap();
    registry
        .register(
            command::<TestArgs, _, _>(
                alpha,
                CommandConf::new("alpha").description("Alpha command."),
            )
            .unwrap(),
        )
        .unwrap();

    let help = registry.execute_help();
    assert!(help.find("alpha").unwrap() < help.find("beta").unwrap());
    assert!(help.contains("Alpha command."));
    assert!(help.contains("Beta command."));

    let command_help = registry.generate_help("alpha").unwrap();
    assert!(command_help.find("--age").unwrap() < command_help.find("--name").unwrap());
    assert!(command_help.contains("Alpha command."));
}
