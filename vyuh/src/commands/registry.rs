use indexmap::IndexMap;

use crate::{Site, errors::ErrorSource};

use super::args::CommandArgType;
use super::error::CommandError;
use super::types::{Command, CommandContext};

/// Registry of named CLI commands.
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

    #[allow(dead_code)]
    pub fn with_banner(mut self, banner: String) -> Self {
        self.banner = Some(banner);
        self
    }

    pub fn merge(&mut self, other: CommandRegistry) -> Result<(), CommandError> {
        for (name, cmd) in other.commands {
            if self.commands.contains_key(&name) {
                return Err(CommandError::AlreadyExists(name));
            }
            self.commands.insert(name, cmd);
        }
        Ok(())
    }

    pub(crate) fn register(&mut self, command: Command) -> Result<(), CommandError> {
        let name = command
            .options
            .name
            .as_deref()
            .unwrap_or_else(|| command.handler.inspect().name.as_str())
            .to_string();
        if name == "help" || self.commands.contains_key(&name) {
            return Err(CommandError::AlreadyExists(name));
        }
        self.commands.insert(name, command);
        Ok(())
    }

    #[allow(dead_code)]
    pub fn iter_commands(&self) -> impl Iterator<Item = &Command> {
        self.commands.values()
    }

    pub fn generate_help(&self, command_name: &str) -> Result<String, CommandError> {
        let command = self
            .commands
            .get(command_name)
            .ok_or_else(|| CommandError::NotFound(command_name.to_string()))?;

        let mut help = format!("Usage: {} [OPTIONS]\n", command_name);
        if let Some(description) = command_summary(command) {
            help.push_str(&format!("\n{}\n", description));
        }
        help.push_str("\nOptions:\n");
        let mut args = command.args.iter().collect::<Vec<_>>();
        args.sort_by(|a, b| a.name.cmp(&b.name));
        for arg in args {
            let required_str = if arg.required { " (required)" } else { "" };
            let desc_str = arg
                .description
                .as_deref()
                .map(|d| format!(" - {}", d))
                .unwrap_or_default();
            let hints_str = if arg.hints.is_empty() {
                String::new()
            } else {
                format!(" [{}]", arg.hints.join("; "))
            };
            let line = if matches!(arg.arg_type, CommandArgType::Boolean) {
                format!(
                    "  --{} / --no-{}{}{}{}\n",
                    arg.name, arg.name, required_str, hints_str, desc_str
                )
            } else {
                let value = if matches!(arg.arg_type, CommandArgType::Array(_)) {
                    format!("<{}>...", arg.arg_type.type_name())
                } else {
                    format!("<{}>", arg.arg_type.type_name())
                };
                format!(
                    "  --{} {}{}{}{}\n",
                    arg.name, value, required_str, hints_str, desc_str
                )
            };
            help.push_str(&line);
        }
        Ok(help)
    }

    pub(crate) fn execute_help(&self) -> String {
        let mut help = String::new();
        if let Some(banner) = &self.banner {
            help.push_str(banner);
            help.push_str("\n\n");
        }
        help.push_str("Available commands:\n\n");
        let mut commands = self.commands.iter().collect::<Vec<_>>();
        commands.sort_by(|(left, _), (right, _)| left.cmp(right));
        for (name, cmd) in commands {
            let summary =
                command_summary(cmd).unwrap_or_else(|| "No description available".to_string());
            help.push_str(&format!("  {:<20} {}\n", name, summary));
        }
        help.push_str("\nUse '<command> --help' for more information on a specific command.\n");
        help
    }

    pub async fn execute(
        &self,
        command_name: &str,
        args: &[&str],
        site: Site,
    ) -> Result<(), CommandError> {
        if command_name == "help" {
            println!("{}", self.execute_help());
            return Ok(());
        }
        if args.contains(&"--help") {
            println!("{}", self.generate_help(command_name)?);
            return Ok(());
        }
        let command = self
            .commands
            .get(command_name)
            .ok_or_else(|| CommandError::UnknownCommand(command_name.to_string()))?;
        let payload = (command.parser)(command_name, args, &command.args)?;
        let ctx = CommandContext::new(site, payload);
        command
            .handler
            .call(ctx)
            .await
            .map(|_| ())
            .map_err(|err| match &err.source {
                Some(ErrorSource::Validation(report)) => CommandError::Validation(report.clone()),
                _ => CommandError::Handler(err),
            })
    }
}

pub(crate) fn builtin_registry() -> Result<CommandRegistry, CommandError> {
    super::core::core_registry()
}

fn command_summary(command: &Command) -> Option<String> {
    command.options.description.clone().or_else(|| {
        command
            .handler
            .inspect()
            .description
            .as_ref()
            .and_then(|d| d.lines().next().map(|s| s.to_string()))
    })
}
