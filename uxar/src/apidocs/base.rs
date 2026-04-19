use std::borrow::Cow;

/// Field nature for database column mapping.
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
    pub reference_from: Option<Cow<'static, str>>,
    pub reference_to: Option<Cow<'static, str>>,
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
            reference_from: None,
            reference_to: None,
            serial: false,
            default: None,
            index: false,
            index_type: None,
            unique: false,
            unique_groups: Vec::new(),
        }
    }
}

/// Table metadata for database mapping.
#[derive(Debug, Clone, Default)]
pub struct TableMeta{
    pub name: Option<Cow<'static, str>>,
}