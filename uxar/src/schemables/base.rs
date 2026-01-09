use std::borrow::Cow;
use openapiv3;
use std::collections::{HashMap, BTreeMap};


pub trait Schemable{
    fn schema_type() -> SchemaType;
}


#[derive(Debug, Clone)]
pub struct StructSchema{
    pub name: Cow<'static, str>,
    pub fields: Vec<SchemaField>,
    pub about: Option<Cow<'static, str>>,
    pub table: TableMeta,
    pub tags: Vec<Cow<'static, str>>,
}

impl StructSchema {
    pub fn table_name(&self) -> &str {
        self.table.name.as_deref().unwrap_or_else(|| self.name.as_ref())
    }
}

#[derive(Debug, Clone)]
pub enum SchemaType{
    Int{
        bits: u8,        
    },
    Str{
        width: Option<usize>,
    },
    Bool,
    Float{bits: u8},
    Optional{inner: Box<SchemaType>},
    List{item: Box<SchemaType>},
    Map{
        value: Box<SchemaType>,
    },
    Struct(StructSchema),
    // To be expanded later
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Nature{
    #[default]
    Normal,
    Flatten,
    Json,
    Reference,
}


#[derive(Debug, Clone)]
pub struct ColumnMeta{
    pub name: Option<Cow<'static, str>>,
    pub primary_key: bool,
    pub serial: bool,
    pub skip: Option<bool>,
    pub nature: Option<Nature>,
    pub default: Option<Cow<'static, str>>,
    pub index: bool,
    pub index_type: Option<Cow<'static, str>>,
    pub unique: bool,
    pub unique_groups: Vec<Cow<'static, str>>,
}

impl Default for ColumnMeta {
    fn default() -> Self {
        Self {
            name: None,
            primary_key: false,
            skip: None,
            nature: None,
            serial: false,
            default: None,
            index: false,
            index_type: None,
            unique: false,
            unique_groups: Vec::new(),
        }
    }
}


#[derive(Debug, Clone, Default)]
pub struct TableMeta{
    pub name: Option<Cow<'static, str>>,
    // To be expanded later
}


#[derive(Debug, Clone)]
pub struct SchemaField{
    pub name: Cow<'static, str>,
    pub about: Option<Cow<'static, str>>,
    pub schema_type: SchemaType,
    pub skip: Option<bool>,
    pub nature: Option<Nature>,
    pub constraints: SchemaConstraints,
    pub column_meta: ColumnMeta,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScalarLit {
    Bool(bool),
    Int(i128),
    Float(f64),
    Str(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StringFormat {
    Email,
    PhoneE164,
    Url,
    Uuid,
    Date,
    DateTime,
    IpV4,
    IpV6,
}

#[derive(Debug, Clone, Default)]
pub struct SchemaConstraints {
    pub enumeration: Vec<ScalarLit>,

    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub exact_length: Option<usize>,
    pub pattern: Option<Cow<'static, str>>,
    pub format: Option<StringFormat>,

    pub minimum: Option<ScalarLit>,
    pub maximum: Option<ScalarLit>,
    pub exclusive_minimum: bool,
    pub exclusive_maximum: bool,
    pub multiple_of: Option<ScalarLit>,

    pub min_items: Option<usize>,
    pub max_items: Option<usize>,
    pub unique_items: bool,
}



impl SchemaType {

}