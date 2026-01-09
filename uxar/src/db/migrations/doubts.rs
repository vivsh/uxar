use std::{hash::{Hash, Hasher}, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::db::migrations::actions::Action;

#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    #[error("invalid input provided: {0}")]
    InvalidInput(String),

    #[error("failed to parse input: {input:?}, expected: {expected:?}")]
    ParseError { input: String, expected: ValueKind },

    #[error("internal serialization error: {0}")]
    SerdeError(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValueKind {
    Int,
    Uint,
    String,
    Float,
    Bool,
    Null,
}

impl ValueKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ValueKind::Uint => "unsigned integer",
            ValueKind::Int => "integer",
            ValueKind::String => "string",
            ValueKind::Float => "float",
            ValueKind::Bool => "boolean",
            ValueKind::Null => "null",
        }
    }

    pub fn parse_value(&self, ans: &str) -> Result<Value, PromptError> {
        let s = ans.trim();

        match self {
            ValueKind::Uint => s.parse::<u64>().map(Value::Uint).map_err(|_| PromptError::ParseError {
                input: ans.to_string(),
                expected: ValueKind::Uint,
            }),

            ValueKind::Int => s.parse::<i64>().map(Value::Int).map_err(|_| PromptError::ParseError {
                input: ans.to_string(),
                expected: ValueKind::Int,
            }),

            ValueKind::Float => s.parse::<f64>().map(Value::Float).map_err(|_| PromptError::ParseError {
                input: ans.to_string(),
                expected: ValueKind::Float,
            }),

            ValueKind::String => Ok(Value::String(ans.to_string())),

            ValueKind::Bool => {
                match s.to_lowercase().as_str() {
                    "true" | "1" | "yes" | "y" => Ok(Value::Bool(true)),
                    "false" | "0" | "no" | "n" => Ok(Value::Bool(false)),
                    _ => Err(PromptError::ParseError {
                        input: ans.to_string(),
                        expected: ValueKind::Bool,
                    }),
                }
            }

            ValueKind::Null => {
                // Accept empty input or explicit null tokens.
                if s.is_empty() || matches!(s.to_lowercase().as_str(), "null" | "none" | "nil") {
                    Ok(Value::Null)
                } else {
                    Err(PromptError::ParseError {
                        input: ans.to_string(),
                        expected: ValueKind::Null,
                    })
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Int(i64),
    Uint(u64),
    String(String),
    Bool(bool),
    Float(f64),
    Null,
}

impl Value {
    pub fn kind(&self) -> ValueKind {
        match self {
            Value::Int(_) => ValueKind::Int,
            Value::Uint(_) => ValueKind::Uint,
            Value::String(_) => ValueKind::String,
            Value::Bool(_) => ValueKind::Bool,
            Value::Float(_) => ValueKind::Float,
            Value::Null => ValueKind::Null,
        }
    }
}



#[derive(Debug, Clone)]
pub struct Choice {
    pub key: u64,
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum Prompt {
    Input {
        message: String,
        kind: ValueKind,
    },
    Choice {
        message: String,
        choices: Vec<Choice>,
    },
    Confirmation {
        message: String,
    },
}

impl Prompt {
    pub fn parse_ans(&self, input: &str) -> Result<Value, PromptError> {
        match self {
            Prompt::Input { kind, .. } => kind.parse_value(input),

            Prompt::Choice { choices, .. } => {
                let selected = match ValueKind::Uint.parse_value(input)? {
                    Value::Uint(u) => u,
                    _ => unreachable!("Uint parser returned non-Uint"),
                };

                if choices.iter().any(|c| c.key == selected) {
                    Ok(Value::Uint(selected))
                } else {
                    Err(PromptError::InvalidInput(input.to_string()))
                }
            }

            Prompt::Confirmation { .. } => ValueKind::Bool.parse_value(input),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[repr(u8)]
pub enum DoubtKind {
    // Is it really a rename or merely a drop and add?
    RenameTable,
    // Is it really a rename or merely a drop and add?
    RenameColumn,
    // Is it safe/valid to proceed without additional user intent?
    // (Django-ish prompting often asks for a default; you can evolve this later.)
    AlterNonNullable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Doubt {
    pub kind: DoubtKind,
    pub action: Arc<Action>,
    hash_code: u64,
}

impl Doubt {

    pub fn new(kind: DoubtKind, action: Action) -> Self {
        let mut hasher = twox_hash::XxHash64::with_seed(0);
        kind.hash(&mut hasher);
        action.hash(&mut hasher);
        let hash_code = hasher.finish();

        Self {
            kind,
            action: Arc::new(action),
            hash_code,
        }
    }
    
    fn kind_tag(&self) -> &'static str {
        match self.kind {
            DoubtKind::RenameTable => "rename_table",
            DoubtKind::RenameColumn => "rename_column",
            DoubtKind::AlterNonNullable => "alter_non_nullable",
        }
    }

    /// Stable key representing this doubt instance.
    /// Uses canonical serialization + hash (NOT Debug formatting).
    pub fn answer_key(&self) -> u64 {
        self.hash_code
    }

    /// Human-friendly action summary (avoid dumping full Debug output).
    fn action_summary(action: &Action) -> String {
        // Keep this intentionally short; can be made richer later.
        match action {
            Action::CreateTable { model } => format!("CreateTable({})", model.name()),
            Action::DropTable { name, .. } => format!("DropTable({})", name),
            Action::AlterTable { name, .. } => format!("AlterTable({})", name),
            Action::RenameTable { old_name, new_name, .. } => {
                format!("RenameTable({} -> {})", old_name, new_name)
            }

            Action::CreateColumn { table, model } => {
                format!("CreateColumn({}.{})", table, model.name)
            }
            Action::DropColumn { table, name } => {
                format!("DropColumn({}.{})", table, name)
            }
            Action::AlterColumn { table, name, delta } => {
                let new_name = delta.name.as_deref().unwrap_or("<same>");
                format!("AlterColumn({}.{} -> {})", table, name, new_name)
            }
            Action::RenameColumn { table, old_name, new_name, .. } => {
                format!("RenameColumn({}.{} -> {})", table, old_name, new_name)
            }

            Action::CreateOpaque { model } => {
                format!("CreateOpaque({:?}:{})", model.kind, model.name())
            }
            Action::DropOpaque { kind, name } => format!("DropOpaque({:?}:{})", kind, name),
            Action::ReplaceOpaque { model } => {
                format!("ReplaceOpaque({:?}:{})", model.kind, model.name())
            }

            Action::Statement { sql } => {
                let first = sql.lines().next().unwrap_or("").trim();
                if first.is_empty() {
                    "Statement(<empty>)".to_string()
                } else if first.len() > 80 {
                    format!("Statement({}…)", &first[..80])
                } else {
                    format!("Statement({})", first)
                }
            }
        }
    }

    /// Create a prompt to disambiguate an action.
    /// Similar to Django-style migration prompts (kept conservative for v0).
    pub fn create_prompt(&self) -> Prompt {
        let summary = Self::action_summary(&self.action);

        match self.kind {
            DoubtKind::RenameTable => Prompt::Confirmation {
                message: format!("Detected possible table rename: {summary}. Confirm? (y/n): "),
            },

            DoubtKind::RenameColumn => Prompt::Confirmation {
                message: format!("Detected possible column rename: {summary}. Confirm? (y/n): "),
            },

            DoubtKind::AlterNonNullable => Prompt::Confirmation {
                message: format!(
                    "This change tightens nullability: {summary}. Proceed? (y/n): "
                ),
            },
        }
    }
}