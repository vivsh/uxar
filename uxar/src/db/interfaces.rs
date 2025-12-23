
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
    pub selectable: bool,
    pub insertable: bool,
    pub updatable: bool,
    pub validation: Option<ColumnValidation>,
}

impl ColumnSpec {
    pub const fn default() -> Self {
        Self {
            kind: ColumnKind::Scalar,
            name: "",
            db_column: "",
            selectable: true,
            insertable: true,
            updatable: true,
            validation: None,
        }
    }

    pub fn can_select(&self) -> bool {
        self.selectable && matches!(self.kind, ColumnKind::Scalar | ColumnKind::Json)
    }

    pub fn can_insert(&self) -> bool {
        self.insertable && matches!(self.kind, ColumnKind::Scalar | ColumnKind::Json)
    }

    pub fn can_update(&self) -> bool {
        self.updatable && matches!(self.kind, ColumnKind::Scalar | ColumnKind::Json)
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
    fn filter_query<B>(&self, qs: Query<B>) -> Query<B>;
}

pub trait Schemable: SchemaInfo + Scannable + Bindable{
    fn query()-> Query<Self> {Query::new()}
}

impl <T: SchemaInfo + Scannable + Bindable> Schemable for T {
    
}

pub trait Recordable {
    fn into_table_model() -> TableModel;
}

pub fn rust_to_pg_type<T: sqlx::Type<Postgres>>() -> String {
    T::type_info().name().to_string()
}
