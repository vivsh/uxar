
use std::{sync::Arc};

use super::commons::{Database, Arguments};
use sqlx::Arguments as _;


#[derive(Clone)]
pub struct ArgValue {
    binder: Arc<dyn Fn(&mut Arguments<'_>) -> Result<(), sqlx::error::BoxDynError> + Send + Sync>,
}

impl ArgValue {
    pub fn new<T>(val: T) -> Self
    where
        T: for<'q> sqlx::Encode<'q, Database> + sqlx::Type<Database> + Send + Sync + 'static,
    {
        Self {
            binder: Arc::new(move |args| {
                use sqlx::Arguments as _;
                args.add(&val)
            }),
        }
    }

    pub fn bind_value(&self, args: &mut Arguments<'_>) -> Result<(), sqlx::error::BoxDynError> {
        (self.binder)(args)
    }
}

impl<T> From<T> for ArgValue
where
    T: for<'q> sqlx::Encode<'q, Database> + sqlx::Type<Database> + Send + Sync + 'static,
{
    fn from(val: T) -> Self {
        ArgValue::new(val)
    }
}

