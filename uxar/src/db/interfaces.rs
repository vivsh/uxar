
use std::borrow::Cow;
use std::collections::HashMap;

use sqlx::Postgres;
use sqlx::TypeInfo;

use crate::db::Statement;
use crate::schemables::Schemable;
use crate::schemables::StructSchema;



pub trait Scannable: Sized{
    fn scan_row_ordered(row: &crate::db::PgRow, start_idx: &mut usize) -> Result<Self, crate::db::SqlxError>;

    fn scan_row(row: &crate::db::PgRow) -> Result<Self, crate::db::SqlxError> {
        let mut idx = 0;
        Self::scan_row_ordered(row, &mut idx)
    }
}


pub trait Bindable: Sized {
    fn bind_values(&self, args: &mut crate::db::PgArguments) -> Result<(), crate::db::SqlxError>;
}

pub trait Filterable {
    fn apply_filters(self, qs: Statement) -> Statement;
}

pub trait Model: Schemable + Scannable + Bindable{

  fn model_schema() -> &'static StructSchema;

}

// impl <T: SchemaInfo + Scannable + Bindable> Model for T {
    
// }

pub fn rust_to_pg_type<T: sqlx::Type<Postgres>>() -> String {
    T::type_info().name().to_string()
}



pub trait Relation<From: Scannable, To: Scannable> {       
    fn join(&self, from: &From, to: &To) -> bool;
}

