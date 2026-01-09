use std::collections::HashMap;

use indexmap::IndexMap;

pub type CommandHandler =
    Box<dyn Fn(&[&str]) -> Result<(), CommandError> + Send + Sync>;

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

    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

pub struct CommandEntry {
    pub description: &'static str,
    pub handler: CommandHandler,
}

pub struct CommandRegistry {
    commands: IndexMap<String, CommandEntry>,
    root: Vec<String>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: IndexMap::new(),
            root: vec!["uxar".to_string()],
        }
    }

    /// Controls the "command name path" shown in argh help output.
    /// Example: ["uxar"] or ["cargo", "uxar"].
    pub fn with_root(mut self, root: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.root = root.into_iter().map(Into::into).collect();
        self
    }

    fn handler_wrapper<T, F>(&self, name: &str, handler: F) -> CommandHandler
    where
        T: argh::FromArgs,
        F: Fn(T) -> Result<(), CommandError> + Send + Sync + 'static,
    {
        let mut command_name = self.root.clone();
        command_name.push(name.to_string());

        Box::new(move |args: &[&str]| {
            let name_refs: Vec<&str> = command_name.iter().map(|s| s.as_str()).collect();
            let parsed = T::from_args(&name_refs, args).map_err(CommandError::Exit)?;
            handler(parsed)
        })
    }

    fn banner(&self) -> String {
        format!("Uxar CLI - Commands available under: {}", self.root.join(" "))
    }

    pub fn command_help(&self) -> String {
        let mut names: Vec<&String> = self.commands.keys().collect();
        names.sort();

        let mut help_text = String::from("Available commands:\n");
        for name in names {
            let entry = &self.commands[name];
            help_text.push_str(&format!("  {:<16} {}\n", name, entry.description));
        }

        help_text.push_str("\nUse: <command> --help  (for per-command help)\n");
        help_text.push_str("Or:  help <command>\n");
        help_text
    }

    /// Register a command with a one-line description shown in `help`.
    pub fn register<T, F>(
        &mut self,
        name: &str,
        description: &'static str,
        handler: F,
    ) -> Result<(), CommandError>
    where
        T: argh::FromArgs,
        F: Fn(T) -> Result<(), CommandError> + Send + Sync + 'static,
    {
        if self.commands.contains_key(name) {
            return Err(CommandError::AlreadyExists(name.to_string()));
        }

        let wrapped = self.handler_wrapper::<T, F>(name, handler);
        self.commands.insert(
            name.to_string(),
            CommandEntry {
                description,
                handler: wrapped,
            },
        );
        Ok(())
    }

    pub fn run_command(&self, name: &str, args: &[&str]) -> Result<(), CommandError> {
        match self.commands.get(name) {
            Some(entry) => (entry.handler)(args),
            None => Err(CommandError::NotFound(name.to_string())),
        }
    }

    /// Invoke commands like a typical CLI dispatcher.
    ///
    /// - No args => prints banner + command list
    /// - `help` or `help <cmd>` => prints list or per-command help
    /// - `cmd --help` => handled by argh (via EarlyExit)
    ///
    /// This function prints output and returns; if you want exit codes,
    /// wrap it in your `main()` and call `std::process::exit(code)`.
    pub fn execute(&self, argv: &[&str]) {
        if argv.is_empty() {
            eprintln!("{}", self.banner());
            eprintln!("{}", self.command_help());
            return;
        }

        let cmd = argv[0];
        let rest = &argv[1..];

        if cmd == "help" {
            if rest.is_empty() {
                println!("{}", self.command_help());
            } else {
                // Show per-command help by invoking that command with `--help`.
                let target = rest[0];
                match self.run_command(target, &["--help"]) {
                    Ok(_) => {}
                    Err(CommandError::Exit(early)) => {
                        // argh already formats help/error output.
                        eprintln!("{}", early.output);
                    }
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            return;
        }

        match self.run_command(cmd, rest) {
            Ok(_) => {}
            Err(CommandError::Exit(early)) => {
                // argh uses EarlyExit for both help and parse errors.
                // Print its output and let the caller decide whether to exit non-zero.
                eprintln!("{}", early.output);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                eprintln!();
                eprintln!("{}", self.command_help());
            }
        }
    }
}