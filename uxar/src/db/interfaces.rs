

use std::hash::Hash;
use std::marker::PhantomData;

use sqlx::TypeInfo;

use crate::db::QuerySet;
use crate::db::{Row, Arguments, Database};


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum RecordKey<'a>{
    String(&'a str),
    Int(u64),
    Uuid(uuid::Uuid),
}

impl<'a> RecordKey<'a>{

    pub fn from_str(s: &'a str) -> Self {
        RecordKey::String(s)
    }

    pub fn from_int(i: u64) -> Self {
        RecordKey::Int(i)
    }

    pub fn from_uuid(u: uuid::Uuid) -> Self {
        RecordKey::Uuid(u)
    }

}

impl <'a> From<&'a str> for RecordKey<'a>{
    fn from(s: &'a str) -> Self {
        RecordKey::String(s)
    }
}

impl From<i64> for RecordKey<'_>{
    fn from(i: i64) -> Self {
        RecordKey::Int(i as u64)
    }
}

impl From<uuid::Uuid> for RecordKey<'_>{
    fn from(u: uuid::Uuid) -> Self {
        RecordKey::Uuid(u)
    }
}

pub struct RecordColumn{
    pub name: &'static str,
    pub type_name: &'static str,
    pub nullable: bool,
    pub default: &'static str,
    pub primary_key: bool,
    pub unique: bool,
    pub unique_groups: Vec<&'static str>,
    pub index: bool,
    pub index_type: Option<&'static str>,
}

pub struct RecordTable{
    pub name: &'static str,
    pub key_column: &'static str,
    pub columns: Vec<RecordColumn>,

}

pub trait Recordable: Sized + Send + 'static{   

    fn record_key(&self) -> Option<RecordKey<'_>>;

    fn record_table() -> &'static RecordTable;

    fn bind_values<'q>(&'q self, args: &mut Arguments<'q>) -> Result<(), crate::db::sqlx::Error>;
    
    fn scan_row_ordered(row: &Row, start_idx: &mut usize) -> Result<Self, crate::db::sqlx::Error>;

    fn scan_row(row: &Row) -> Result<Self, crate::db::sqlx::Error> {
        let mut idx = 0;
        Self::scan_row_ordered(row, &mut idx)
    }
}


pub trait Scannable: Sized  {    

    fn scan_column_names() -> Vec<String>;

    fn scan_row_ordered(row: &Row, start_idx: &mut usize) -> Result<Self, crate::db::sqlx::Error>;
    
    fn scan_row_unordered(row: &Row) -> Result<Self, crate::db::sqlx::Error>;

    fn scan_row(row: &Row) -> Result<Self, crate::db::sqlx::Error> {
        let mut idx = 0;
        Self::scan_row_ordered(row, &mut idx)
    }
}



pub trait Bindable {    

    fn bind_values<'q>(&'q self, args: &mut Arguments<'q>) -> Result<(), crate::db::sqlx::Error>;
    
    fn bind_column_names() -> Vec<String>;
}

pub trait Model: Scannable + Bindable{

    type PrimaryKey: Hash + Eq;

    fn primary_key(&self) -> Self::PrimaryKey;

    fn primary_key_column() -> &'static str;
}

pub trait Filterable {
    fn apply_filters(self, qs: QuerySet) -> QuerySet;
}
    

pub fn rust_to_pg_type<T: sqlx::Type<Database>>() -> String {
    T::type_info().name().to_string()
}



/// Defines a one-to-may or one-to-one relation between two models 
pub trait PrefetchRelation {    

    type To: Scannable;    
    
    fn to_key(&self, to: &Self::To) -> Self::Key;

    type Key: std::hash::Hash + Eq;   

    type From: Scannable + Send;

    fn from_key_column(&self)-> &'static str;

    fn from_key(&self, from: &Self::From) -> Self::Key;

    fn to_key_column(&self)-> &'static str;

    fn attach(&self, from: &mut Self::From, to: Self::To);
}
