use std::hash::Hash;

use crate::db::{Arguments, Row};

pub trait Scannable: Sized {
    fn scan_column_names() -> Vec<String>;

    fn scan_row_ordered(row: &Row, start_idx: &mut usize) -> Result<Self, sqlx::Error>;

    fn scan_row_unordered(row: &Row) -> Result<Self, sqlx::Error>;

    fn scan_row(row: &Row) -> Result<Self, sqlx::Error> {
        let mut idx = 0;
        Self::scan_row_ordered(row, &mut idx)
    }
}

pub trait Bindable {
    fn bind_values(&self, args: &mut Arguments<'static>) -> Result<(), sqlx::Error>;

    fn bind_column_names() -> Vec<String>;
}

pub trait Model: Scannable + Bindable {
    type PrimaryKey: Hash + Eq;

    fn primary_key(&self) -> Self::PrimaryKey;

    fn primary_key_column() -> &'static str;
}
