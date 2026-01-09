use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::db::{ColumnModel, TableModel, models::{ColumnDelta, OpaqueModel, OpaqueType, QualifiedName, StrRef, TableDelta}};




#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq, Eq)]
pub enum Action {
    CreateTable {
        model: Arc<TableModel>,
    },
    DropTable {
        name: QualifiedName,
    },
    AlterTable {
        delta: TableDelta,
        name: QualifiedName,
    },
    RenameTable {
        old_name: QualifiedName,
        new_name: QualifiedName,
    },
    CreateColumn {
        table: QualifiedName,
        model: Arc<ColumnModel>,
    },
    DropColumn {
        table: QualifiedName,
        name: StrRef,
    },
    AlterColumn {
        table: QualifiedName,
        name: StrRef,
        delta: ColumnDelta,
    },
    RenameColumn {
        table: QualifiedName,
        old_name: StrRef,
        new_name: StrRef,
    },
    CreateOpaque {
        model: Arc<OpaqueModel>,
    },
    DropOpaque {
        name: QualifiedName,
        kind: OpaqueType,
    },
    ReplaceOpaque {
        model: Arc<OpaqueModel>,
    },
    Statement {
        sql: StrRef,
    },
}

impl Action{
    /// Returns a human-readable description for use in disambiguating prompts.
    /// Format mirrors Django's migration action descriptions.
    pub fn describe(&self)->String{
        match self {
            Action::CreateTable { model } => {
                format!("Create table '{}'", model.qualified_name())
            }
            Action::DropTable { name } => {
                format!("Drop table '{}'", name)
            }
            Action::AlterTable { name, .. } => {
                format!("Alter table '{}'", name)
            }
            Action::RenameTable { old_name, new_name } => {
                format!("Rename table '{}' to '{}'", old_name, new_name)
            }

            Action::CreateColumn { table, model } => {
                format!("Create column '{}.{}'", table, model.name)
            }
            Action::DropColumn { table, name } => {
                format!("Drop column '{}.{}'", table, name)
            }
            Action::AlterColumn { table, name, delta } => {
                let new_name = delta.name.as_deref().unwrap_or(name);
                if new_name != &**name {
                    format!("Alter column '{}.{}' (rename to '{}')", table, name, new_name)
                } else {
                    format!("Alter column '{}.{}'", table, name)
                }
            }
            Action::RenameColumn { table, old_name, new_name } => {
                format!("Rename column '{}.{}' to '{}'", table, old_name, new_name)
            }

            Action::CreateOpaque { model } => {
                format!("Create {:?} '{}'", model.kind, model.qualified_name())
            }
            Action::DropOpaque { kind, name } => {
                format!("Drop {:?} '{}'", kind, name)
            }
            Action::ReplaceOpaque { model } => {
                format!("Replace {:?} '{}'", model.kind, model.qualified_name())
            }
            Action::Statement { sql } => {
                let preview = if sql.len() > 60 {
                    format!("{}...", &sql[..60])
                } else {
                    sql.to_string()
                };
                format!("Execute SQL: {}", preview)
            }
        }
    }

    /// Validate this action (opt-in validation)
    pub fn validate(&self) -> Result<(), crate::db::models::ValidationError> {
        use crate::db::models::ValidationError;
        
        match self {
            Action::CreateTable { model } => model.validate(),
            Action::DropTable { name } | Action::AlterTable { name, .. } => {
                name.validate()?;
                Ok(())
            }
            Action::CreateColumn { table, model } => {
                table.validate()?;
                model.validate()
            }
            Action::DropColumn { table, .. } | 
            Action::AlterColumn { table, .. } | 
            Action::RenameColumn { table, .. } => {
                table.validate()?;
                Ok(())
            }
            Action::CreateOpaque { model } | Action::ReplaceOpaque { model } => model.validate(),
            Action::DropOpaque { name, .. } => {
                name.validate()?;
                Ok(())
            }
            Action::Statement { sql } => {
                if sql.is_empty() {
                    return Err(ValidationError::EmptyName);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Generates a slug for this action, Django-style.
    /// Used for creating descriptive migration IDs.
    fn to_slug(&self) -> String {
        match self {
            Action::CreateTable { model } => {
                format!("create_{}", sanitize_name(model.name()))
            }
            Action::DropTable { name } => {
                format!("drop_{}", sanitize_name(&name.full_name()))
            }
            Action::AlterTable { name, .. } => {
                format!("alter_{}", sanitize_name(&name.full_name()))
            }
            Action::RenameTable { old_name, new_name } => {
                format!("rename_{}_{}", sanitize_name(&old_name.full_name()), sanitize_name(&new_name.full_name()))
            }
            Action::CreateColumn { table, model } => {
                format!("add_{}_{}", sanitize_name(&table.full_name()), sanitize_name(&model.name))
            }
            Action::DropColumn { table, name } => {
                format!("remove_{}_{}", sanitize_name(&table.full_name()), sanitize_name(name))
            }
            Action::AlterColumn { table, name, delta } => {
                let new_name = delta.name.as_deref().unwrap_or(name);
                if new_name != &**name {
                    format!("rename_{}_{}_{}", sanitize_name(&table.full_name()), sanitize_name(name), sanitize_name(new_name))
                } else {
                    format!("alter_{}_{}", sanitize_name(&table.full_name()), sanitize_name(name))
                }
            }
            Action::RenameColumn { table, old_name, new_name } => {
                format!("rename_{}_{}_{}", sanitize_name(&table.full_name()), sanitize_name(old_name), sanitize_name(new_name))
            }
            Action::CreateOpaque { model } => {
                format!("create_{}", sanitize_name(model.name()))
            }
            Action::DropOpaque { name, .. } => {
                format!("drop_{}", sanitize_name(&name.full_name()))
            }
            Action::ReplaceOpaque { model } => {
                format!("replace_{}", sanitize_name(model.name()))
            }
            Action::Statement { .. } => {
                "custom".to_string()
            }
        }
    }
}

/// Generate a migration name slug from operations, Django-style.
/// Examples:
/// - [CreateTable(users)] → "create_users"
/// - [CreateTable(users), CreateTable(posts)] → "create_users_and_more"
/// - [AddColumn(users, email), AlterColumn(users, name)] → "add_users_email_and_more"
pub fn generate_migration_name(operations: &[super::migrator::Operation]) -> String {
    if operations.is_empty() {
        return "empty".to_string();
    }

    let slugs: Vec<String> = operations.iter().map(|op| op.action.to_slug()).collect();
    
    if slugs.len() == 1 {
        slugs[0].clone()
    } else if slugs.len() == 2 {
        format!("{}_{}", slugs[0], slugs[1])
    } else {
        // Multiple operations: use first + "_and_more" like Django
        format!("{}_and_more", slugs[0])
    }
}

/// Sanitize a name for use in migration file names.
/// Converts to lowercase, replaces non-alphanumeric with underscores.
/// Bounds length to 50 characters to prevent filesystem issues.
fn sanitize_name(name: &str) -> String {
    let sanitized = name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    
    // Bound to 50 chars to prevent overly long filenames
    if sanitized.len() > 50 {
        sanitized[..50].to_string()
    } else {
        sanitized
    }
}
