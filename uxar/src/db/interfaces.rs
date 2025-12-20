
use crate::db::{query::Query};

#[derive(Debug, Clone)]
pub enum ColumnKind {
    Scalar,
    Flatten { columns: &'static [ColumnSpec] },
    Json,
    Reference { columns: &'static [ColumnSpec] },
}

#[derive(Debug, Clone)]
pub struct ColumnSpec {
    pub kind: ColumnKind,
    pub name: &'static str,
    pub db_column: &'static str,
    pub selectable: bool,
    pub insertable: bool,
    pub updatable: bool,
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

pub trait Schemable {
    fn schema() -> &'static [ColumnSpec];
}

pub trait Scannable: Sized{
    fn scan_row_ordered(row: &sqlx::postgres::PgRow, start_idx: &mut usize) -> Result<Self, sqlx::Error>;

    fn scan_row(row: &sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let mut idx = 0;
        Self::scan_row_ordered(row, &mut idx)
    }

    fn select_from(source: &str) -> Query where Self: Schemable {
        let mut qs = Query::new();
        qs = qs.push_select::<Self>(source, "");
        qs
    }
}


pub trait Bindable: Schemable + Sized {

    fn bind_values(&self, args: &mut sqlx::postgres::PgArguments) -> Result<(), sqlx::Error>;

    fn insert_into(&self, source: &str) -> Query {
        let mut qs = Query::new();
        qs = qs.push_insert::<Self>(source, self);
        qs
    }

    fn update_into(&self, source: &str) -> Query {
        let mut qs = Query::new();
        qs = qs.push_update::<Self>(source, self);
        qs
    }
}

pub trait Filterable {
    fn filter_query(&self, qs: Query) -> Query;
}

