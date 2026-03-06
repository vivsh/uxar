use crate::db::placeholders::has_named_placeholder;

use super::commons::Database;
use super::argvalue::ArgValue;
use indexmap::IndexMap;

pub struct Scope {
    pub(crate) expr: String,
    pub(crate) named_args: IndexMap<String, ArgValue>,
    pub(crate) has_named_placeholders: bool,
}

impl Scope {
    pub fn new(expr: impl Into<String>) -> Self {
        let expr = expr.into();
        Self {
            has_named_placeholders: has_named_placeholder(&expr),
            expr,
            named_args: IndexMap::new(),
        }
    }

    pub fn bind<T>(mut self, name: &str, val: T) -> Self
    where
        T: for<'q> sqlx::Encode<'q, Database> + sqlx::Type<Database> + Send + Sync +'static,
    {
        self.named_args.insert(name.to_string(), ArgValue::new(val));
        self
    }
}

