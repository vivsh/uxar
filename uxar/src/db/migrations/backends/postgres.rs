use crate::db::migrations::actions::Action;
use crate::db::models::{FkAction, OpaqueType};
use super::base::{ActionToSQL, SqlGenError};

/// PostgreSQL SQL generator for migration actions
pub struct PostgresBackend;

impl PostgresBackend {
    pub fn new() -> Self {
        Self
    }

    /// Quote identifier (table/column name) for safe SQL
    fn quote_ident(&self, name: &str) -> String {
        // Postgres uses double quotes for identifiers
        format!("\"{}\"", name.replace("\"", "\"\""))
    }

    /// Format a qualified table name (schema.table or table)
    fn format_table_name(&self, schema: &Option<String>, table: &str) -> String {
        match schema {
            Some(s) => format!("{}.{}", self.quote_ident(s), self.quote_ident(table)),
            None => self.quote_ident(table),
        }
    }

    /// Build column definition for CREATE/ALTER
    fn build_column_def(&self, col: &crate::db::ColumnModel) -> String {
        let mut parts = vec![self.quote_ident(&col.name)];
        
        // Data type with optional width
        let type_str = match col.width {
            Some(w) => format!("{}({})", col.data_type.to_uppercase(), w),
            None => col.data_type.to_uppercase(),
        };
        parts.push(type_str);
        
        // Nullable constraint
        if !col.is_nullable {
            parts.push("NOT NULL".to_string());
        }
        
        // Default value
        if let Some(ref default) = col.default {
            parts.push(format!("DEFAULT {}", default));
        }
        
        // Primary key
        if col.primary_key {
            parts.push("PRIMARY KEY".to_string());
        }
        
        // Unique constraint
        if col.unique {
            parts.push("UNIQUE".to_string());
        }
        
        // Check constraint
        if let Some(ref check) = col.check {
            parts.push(format!("CHECK ({})", check));
        }
        
        // Foreign key (inline)
        if let Some(ref fk) = col.foreign_key {
            let mut fk_clause = format!("REFERENCES {}", fk.table);
            if !fk.column.is_empty() {
                fk_clause.push_str(&format!(" ({})", self.quote_ident(&fk.column)));
            }
            if let Some(ref on_delete) = fk.on_delete {
                fk_clause.push_str(&format!(" ON DELETE {}", self.format_fk_action(on_delete)));
            }
            parts.push(fk_clause);
        }
        
        parts.join(" ")
    }

    fn format_fk_action(&self, action: &FkAction) -> &str {
        match action {
            FkAction::Cascade => "CASCADE",
            FkAction::SetNull => "SET NULL",
            FkAction::Restrict => "RESTRICT",
            FkAction::NoAction => "NO ACTION",
        }
    }
}
