
use std::collections::HashMap;

use sqlx::Postgres;
use sqlx::TypeInfo;

use crate::db::{models::TableModel, query::Query};

#[derive(Debug, Clone)]
pub enum ColumnKind {
    Scalar,
    Flatten { columns: &'static [ColumnSpec] },
    Json,
    Reference { columns: &'static [ColumnSpec] },
}


#[derive(Debug, Clone)]
pub struct ColumnValidation {
    pub email: bool,
    pub url: bool,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub exact_length: Option<usize>,
    pub min_value: Option<i64>,
    pub max_value: Option<i64>,
    pub range: Option<(i64, i64)>,
    pub regex: Option<&'static str>,
    pub non_empty: bool,
    pub alphanumeric: bool,
    pub slug: bool,
    pub digits: bool,
    pub uuid: bool,
    pub ipv4: bool,
}

#[derive(Debug, Clone)]
pub struct ColumnSpec {
    pub kind: ColumnKind,
    pub name: &'static str,
    pub db_column: &'static str,
    pub nullable: bool,
    pub selectable: bool,
    pub insertable: bool,
    pub updatable: bool,
    pub primary_key: bool,
    pub validation: Option<ColumnValidation>,
    
    // DB-specific constraints (only filled when table_name is present)
    pub unique: bool,
    pub unique_group: Option<&'static str>,
    pub db_indexed: bool,
    pub db_index_type: Option<&'static str>,
    pub db_default: Option<&'static str>,
    pub db_check: Option<&'static str>,
}

impl ColumnSpec {
    pub const fn default() -> Self {
        Self {
            kind: ColumnKind::Scalar,
            name: "",
            db_column: "",
            nullable: false,
            selectable: true,
            insertable: true,
            updatable: true,
            primary_key: false,
            validation: None,
            unique: false,
            unique_group: None,
            db_indexed: false,
            db_index_type: None,
            db_default: None,
            db_check: None,
        }
    }

    pub fn is_scalar_or_json(&self) -> bool {
        matches!(self.kind, ColumnKind::Scalar | ColumnKind::Json)
    }

    /// Check if this column should be included in SELECT queries
    pub fn can_select(&self) -> bool {
        self.selectable && self.is_scalar_or_json()
    }

    /// Check if this column should be included in INSERT queries
    pub fn can_insert(&self) -> bool {
        self.insertable && self.is_scalar_or_json()
    }

    /// Check if this column should be included in UPDATE queries
    pub fn can_update(&self) -> bool {
        self.updatable && self.is_scalar_or_json() && !self.primary_key
    }
}

/// Provides schema metadata for database types.
/// 
/// Implemented automatically by `#[derive(Schemable)]`.
pub trait SchemaInfo {
    fn schema() -> &'static [ColumnSpec];
    fn name() -> &'static str;
}

pub trait Scannable: Sized{
    fn scan_row_ordered(row: &crate::db::PgRow, start_idx: &mut usize) -> Result<Self, crate::db::SqlxError>;

    fn scan_row(row: &crate::db::PgRow) -> Result<Self, crate::db::SqlxError> {
        let mut idx = 0;
        Self::scan_row_ordered(row, &mut idx)
    }
}


pub trait Bindable: SchemaInfo + Sized {
    fn bind_values(&self, args: &mut crate::db::PgArguments) -> Result<(), crate::db::SqlxError>;
}

pub trait Filterable {
    fn filter_query(&self, qs: Query) -> Query;
}

pub trait Model: SchemaInfo + Scannable + Bindable{

    fn select() -> Query {
        Query::new().select::<Self>()
    }

    fn insert(item: &Self) -> Query {
        Query::new().insert::<Self>(item)
    }

    fn update(item: &Self) -> Query {
        Query::new().update::<Self>(item)
    }

    fn delete() -> Query {
        Query::new().delete::<Self>()
    }

}

impl <T: SchemaInfo + Scannable + Bindable> Model for T {
    
}

pub fn rust_to_pg_type<T: sqlx::Type<Postgres>>() -> String {
    T::type_info().name().to_string()
}
