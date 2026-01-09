use serde::{Deserialize, Serialize};
use std::{fmt, sync::Arc};

pub type StrRef = Arc<str>;

/// Validation errors for database models (opt-in validation)
#[derive(Debug, Clone, thiserror::Error)]
pub enum ValidationError {
    #[error("name cannot be empty")]
    EmptyName,
    #[error("name '{0}' exceeds maximum length of 63 characters")]
    NameTooLong(String),
    #[error("name '{0}' contains invalid characters")]
    InvalidCharacters(String),
}

pub trait IntoEntity {
    fn to_entity(&self) -> StrRef;

    fn from_entity(entity: &str) -> Self
    where
        Self: Sized;
}

/// A qualified database object name (e.g., "schema.table" or just "table")
#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct QualifiedName {
    schema: Option<StrRef>,
    name: StrRef,
}

impl QualifiedName {
    /// Create a new qualified name (no validation)
    pub fn new(name: StrRef, schema: Option<StrRef>) -> Self {
        Self { schema, name }
    }

    /// Validate this qualified name
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_identifier(&self.name)?;
        if let Some(ref s) = self.schema {
            validate_identifier(s)?;
        }
        Ok(())
    }

    pub fn full_name(&self)->String{
        format!("{}", self)
    }

    pub fn parse(s: &str) -> Self {
        match s.split_once('.') {
            Some((schema, name)) => Self::new(Arc::from(name), Some(Arc::from(schema))),
            None => Self::new(Arc::from(s), None),
        }
    }

    /// Extract schema from qualified name
    pub fn schema(&self) -> Option<&str> {
        self.schema.as_deref()
    }

    /// Extract name (without schema) from qualified name
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for QualifiedName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(s) = self.schema.as_deref() {
            write!(f, "{}.", s)?;
        }
        write!(f, "{}", self.name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct TableModel {
    pub qualified_name: QualifiedName,
    pub columns: Vec<ColumnModel>,
}

impl TableModel {
    /// Create a new table (no validation)
    pub fn new(name: StrRef, schema: Option<StrRef>) -> Self {
        Self {
            qualified_name: QualifiedName::new(name, schema),
            columns: Vec::new(),
        }
    }

    /// Validate this table model
    pub fn validate(&self) -> Result<(), ValidationError> {
        self.qualified_name.validate()?;
        for col in &self.columns {
            col.validate()?;
        }
        Ok(())
    }

    /// Get the qualified name as a string slice
    pub fn qualified_name(&self) -> &QualifiedName {
        &self.qualified_name
    }

    /// Extract table name (without schema) from qualified name
    pub fn name(&self) -> &str {
        self.qualified_name.name()
    }

    pub fn add_column(&mut self, column: ColumnModel) {
        self.columns.push(column);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
// placeholder for now
pub struct TableDelta {}

impl TableDelta {
    pub fn new() -> Self {
        Self {}
    }

    pub fn apply(&self, old_table: &TableModel) -> TableModel {
        let mut table = old_table.clone();
        table
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct ColumnModel {
    pub name: StrRef,
    pub data_type: StrRef,
    pub width: Option<u32>,
    pub is_nullable: bool,
    pub primary_key: bool,
    pub unique: bool,
    pub unique_group: Option<StrRef>,
    pub indexed: bool,
    pub index_type: Option<StrRef>,
    pub default: Option<StrRef>,
    pub check: Option<StrRef>,
    pub foreign_key: Option<ForeignKey>,
}

impl ColumnModel {
    pub fn new(name: StrRef, data_type: StrRef) -> Self {
        Self {
            name,
            data_type,
            width: None,
            is_nullable: false,
            primary_key: false,
            unique: false,
            unique_group: None,
            indexed: false,
            index_type: None,
            default: None,
            check: None,
            foreign_key: None,
        }
    }

    /// Validate this column model
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_identifier(&self.name)?;
        if self.data_type.is_empty() {
            return Err(ValidationError::EmptyName);
        }
        if self.data_type.len() > 255 {
            return Err(ValidationError::NameTooLong(self.data_type.to_string()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct ColumnDelta {
    pub name: Option<StrRef>,
    pub data_type: Option<StrRef>,
    pub width: Option<Option<u32>>,
    pub is_nullable: Option<bool>,
    pub primary_key: Option<bool>,
    pub unique: Option<bool>,
    pub unique_group: Option<Option<StrRef>>,
    pub indexed: Option<bool>,
    pub index_type: Option<Option<StrRef>>,
    pub default: Option<Option<StrRef>>,
    pub check: Option<Option<StrRef>>,
    pub foreign_key: Option<Option<ForeignKey>>,
}

impl ColumnDelta {
    pub fn new() -> Self {
        Self {
            name: None,
            data_type: None,
            width: None,
            is_nullable: None,
            primary_key: None,
            unique: None,
            unique_group: None,
            indexed: None,
            index_type: None,
            default: None,
            check: None,
            foreign_key: None,
        }
    }

    pub fn apply(&self, old_column: &ColumnModel) -> ColumnModel {
        let mut column = old_column.clone();
        if let Some(name) = &self.name {
            column.name = name.clone();
        }
        if let Some(data_type) = &self.data_type {
            column.data_type = data_type.clone();
        }
        if let Some(width) = &self.width {
            column.width = *width;
        }
        if let Some(is_nullable) = self.is_nullable {
            column.is_nullable = is_nullable;
        }
        if let Some(primary_key) = self.primary_key {
            column.primary_key = primary_key;
        }
        if let Some(unique) = self.unique {
            column.unique = unique;
        }
        if let Some(unique_group) = &self.unique_group {
            column.unique_group = unique_group.clone();
        }
        if let Some(indexed) = self.indexed {
            column.indexed = indexed;
        }
        if let Some(index_type) = &self.index_type {
            column.index_type = index_type.clone();
        }
        if let Some(default) = &self.default {
            column.default = default.clone();
        }
        if let Some(check) = &self.check {
            column.check = check.clone();
        }
        if let Some(foreign_key) = &self.foreign_key {
            column.foreign_key = foreign_key.clone();
        }
        column
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum FkAction {
    Cascade,
    SetNull,
    Restrict,
    NoAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct ForeignKey {
    /// Referenced table (qualified name)
    pub table: QualifiedName,
    /// Referenced column
    pub column: StrRef,
    /// Constraint name
    pub name: Option<StrRef>,
    /// ON DELETE behavior
    pub on_delete: Option<FkAction>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum OpaqueType {
    Function,
    TriggerFunction,
    Index,
    View,
    Trigger,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct OpaqueModel {
    pub qualified_name: QualifiedName,
    pub kind: OpaqueType,
    pub definition: StrRef,
    /// For triggers: qualified name of the table this trigger is attached to
    #[serde(default)]
    pub related_table: Option<QualifiedName>,
    /// Tables this opaque depends on (qualified names).
    /// Extracted from definition or manually specified.
    #[serde(default)]
    pub depends_on_tables: Vec<QualifiedName>,
    /// Other opaques this depends on (qualified names).
    /// e.g., a view referencing another view, or a trigger calling a function.
    #[serde(default)]
    pub depends_on_opaques: Vec<QualifiedName>,
}

impl OpaqueModel {
    /// Get the qualified name as a string slice
    pub fn qualified_name(&self) -> &QualifiedName {
        &self.qualified_name
    }

    /// Extract schema from qualified name
    pub fn schema(&self) -> Option<&str> {
        self.qualified_name.schema()
    }

    /// Extract name (without schema) from qualified name
    pub fn name(&self) -> &str {
        self.qualified_name.name()
    }

    /// Validate this opaque model
    pub fn validate(&self) -> Result<(), ValidationError> {
        self.qualified_name.validate()?;
        Ok(())
    }
}

/// Validate identifier (table/column/schema name) - opt-in validation helper
fn validate_identifier(name: &str) -> Result<(), ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::EmptyName);
    }
    if name.len() > 63 {
        return Err(ValidationError::NameTooLong(name.to_string()));
    }
    // PostgreSQL allows letters, digits, underscore, $ and starts with letter or underscore
    let first_char = name.chars().next().ok_or(ValidationError::EmptyName)?;
    if !first_char.is_alphabetic() && first_char != '_' {
        return Err(ValidationError::InvalidCharacters(name.to_string()));
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
        return Err(ValidationError::InvalidCharacters(name.to_string()));
    }
    Ok(())
}
