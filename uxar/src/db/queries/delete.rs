use std::borrow::Cow;
use std::collections::HashMap;

use crate::db::argvalue::ArgValue;
use crate::db::commons::{Arguments, Row};
use crate::db::executor::{DbError, DBSession};
use crate::db::interfaces::{Filterable, Scannable};
use crate::db::placeholders::{has_named_placeholder, resolve_placeholders, Dialect};
use super::{FilteredBuilder, QueryError, Statement};

/// Builder for DELETE queries. Constructed via `db::delete(table)`.
pub struct DeleteQuery {
    source: String,
    filters: Vec<Cow<'static, str>>,
    args: Arguments<'static>,
    named_args: HashMap<String, ArgValue>,
    error: Option<QueryError>,
}

impl DeleteQuery {
    pub(crate) fn new(source: &str) -> Self {
        Self {
            source: source.to_string(),
            filters: Vec::new(),
            args: Arguments::default(),
            named_args: HashMap::new(),
            error: super::validate_ident(source).err(),
        }
    }

    /// Bind a positional argument.
    pub fn bind<T>(mut self, val: T) -> Self
    where
        T: for<'q> sqlx::Encode<'q, crate::db::commons::Database>
            + sqlx::Type<crate::db::commons::Database>
            + Send
            + 'static,
    {
        use sqlx::Arguments as _;
        if self.error.is_some() {
            return self;
        }
        if let Err(e) = self.args.add(val) {
            self.error = Some(QueryError::BindError(e.to_string()));
        }
        self
    }

    /// Bind a named argument.
    pub fn bind_as<T>(mut self, name: &str, val: T) -> Self
    where
        T: for<'q> sqlx::Encode<'q, crate::db::commons::Database>
            + sqlx::Type<crate::db::commons::Database>
            + Send
            + Sync
            + 'static,
    {
        if self.error.is_some() {
            return self;
        }
        self.named_args.insert(name.to_string(), ArgValue::new(val));
        self
    }

    pub fn filter_with<F: Filterable>(self, filter: F) -> Self {
        filter.apply_filters_delete(self)
    }

    // ── internal ──────────────────────────────────────────────────────────────

    fn build_filter_clause(&self) -> String {
        if self.filters.is_empty() {
            return String::new();
        }
        format!(" WHERE {}", self.filters.join(" AND "))
    }

    fn resolve(mut self, sql: String) -> Result<(String, Arguments<'static>), QueryError> {
        if let Some(err) = self.error {
            return Err(err);
        }
        if self.named_args.is_empty() && !has_named_placeholder(&sql) {
            return Ok((sql, self.args));
        }
        let final_sql =
            resolve_placeholders(&sql, &mut self.args, &self.named_args, Dialect::Postgres)?;
        Ok((final_sql, self.args))
    }

    fn into_statement_with_suffix(self, suffix: &str) -> Result<Statement, QueryError> {
        let sql = format!(
            "DELETE FROM {}{}{}",
            self.source,
            self.build_filter_clause(),
            suffix,
        );
        let (final_sql, final_args) = self.resolve(sql)?;
        Ok(Statement::new(&final_sql, final_args))
    }

    // ── terminal methods ──────────────────────────────────────────────────────

    pub async fn execute<S: DBSession>(self, session: &mut S) -> Result<u64, DbError> {
        let stmt = self.into_statement_with_suffix("")?;
        session.execute(stmt).await
    }

    /// Execute and return all deleted rows via RETURNING * (Postgres only).
    #[cfg(feature = "postgres")]
    pub async fn all<M, S>(self, session: &mut S) -> Result<Vec<M>, DbError>
    where
        M: Scannable + for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
        S: DBSession,
    {
        let stmt = self.into_statement_with_suffix(" RETURNING *")?;
        session.fetch_all(stmt).await
    }

    /// Execute and return the first deleted row via RETURNING * (Postgres only).
    #[cfg(feature = "postgres")]
    pub async fn first<M, S>(self, session: &mut S) -> Result<Option<M>, DbError>
    where
        M: Scannable + for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
        S: DBSession,
    {
        let stmt = self.into_statement_with_suffix(" RETURNING *")?;
        session.fetch_optional(stmt).await
    }
}

impl FilteredBuilder for DeleteQuery {
    fn filter(mut self, cond: impl Into<Cow<'static, str>>) -> Self {
        self.filters.push(cond.into());
        self
    }

    fn bind_dyn(mut self, val: ArgValue) -> Self {
        if self.error.is_some() {
            return self;
        }
        if let Err(e) = val.bind_value(&mut self.args) {
            self.error = Some(QueryError::BindError(e.to_string()));
        }
        self
    }

    fn bind_named_dyn(mut self, name: &str, val: ArgValue) -> Self {
        if self.error.is_some() {
            return self;
        }
        self.named_args.insert(name.to_string(), val);
        self
    }
}
