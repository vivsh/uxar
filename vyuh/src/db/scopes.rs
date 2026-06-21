use super::argvalue::ArgValue;
use super::commons::Database;
use indexmap::IndexMap;

#[derive(Clone)]
pub struct Scope {
    pub(crate) expr: String,
    pub(crate) named_args: IndexMap<String, ArgValue>,
}

impl Scope {
    pub fn new(expr: impl Into<String>) -> Self {
        let expr = expr.into();
        Self {
            expr,
            named_args: IndexMap::new(),
        }
    }

    pub fn bind<T>(mut self, name: &str, val: T) -> Self
    where
        T: Clone
            + for<'q> sqlx::Encode<'q, Database>
            + sqlx::Type<Database>
            + Send
            + Sync
            + 'static,
    {
        self.named_args.insert(name.to_string(), ArgValue::new(val));
        self
    }
}
