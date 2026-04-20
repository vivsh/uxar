use std::borrow::Cow;
use std::collections::HashMap;

use crate::db::argvalue::ArgValue;
use crate::db::commons::{Arguments, Row};
use crate::db::executor::{DbError, DBSession};
use crate::db::interfaces::{Bindable, Filterable, Scannable};
use crate::db::placeholders::{has_named_placeholder, resolve_placeholders, Dialect};
use super::{FilteredBuilder, QueryError, Statement};

/// Builder for UPDATE queries. Constructed via `db::update(table)`.
pub struct UpdateQuery {
    source: String,
    filters: Vec<Cow<'static, str>>,
    args: Arguments<'static>,
    named_args: HashMap<String, ArgValue>,
    set_sql: Option<String>,
    error: Option<QueryError>,
}

impl UpdateQuery {
    pub(crate) fn new(source: &str) -> Self {
        Self {
            source: source.to_string(),
            filters: Vec::new(),
            args: Arguments::default(),
            named_args: HashMap::new(),
            set_sql: None,
            error: None,
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

    /// Specify SET columns from a Bindable item.
    pub fn set<M: Bindable>(mut self, item: &M) -> Self {
        use sqlx::Arguments as _;
        if self.error.is_some() {
            return self;
        }
        let cols = M::bind_column_names();
        if cols.is_empty() {
            self.error = Some(QueryError::BindError("no columns to update".to_string()));
            return self;
        }
        let before = self.args.len();
        if let Err(e) = item.bind_values(&mut self.args) {
            self.error = Some(QueryError::BindError(e.to_string()));
            return self;
        }
        let bound = self.args.len().saturating_sub(before);
        if bound != cols.len() {
            self.error = Some(QueryError::BindCountMismatch {
                expected: cols.len(),
                got: bound,
            });
            return self;
        }
        let set_parts: Vec<String> = cols
            .iter()
            .enumerate()
            .map(|(i, col)| format!("{} = {}", col, placeholder_at(before + i)))
            .collect();
        self.set_sql = Some(set_parts.join(", "));
        self
    }

    pub fn filter_with<F: Filterable>(self, filter: F) -> Self {
        filter.apply_filters_update(self)
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
        if let Some(ref err) = self.error {
            return Err(err.clone());
        }
        let set_sql = self.set_sql.as_deref().ok_or_else(|| {
            QueryError::BindError(
                "no SET data — call .set(item) before executing".to_string(),
            )
        })?;
        let sql = format!(
            "UPDATE {} SET {}{}{}",
            self.source,
            set_sql,
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

    /// Execute and return updated rows via RETURNING * (Postgres only).
    #[cfg(feature = "postgres")]
    pub async fn one<M, S>(self, session: &mut S) -> Result<M, DbError>
    where
        M: Scannable + for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
        S: DBSession,
    {
        let stmt = self.into_statement_with_suffix(" RETURNING *")?;
        session.fetch_one(stmt).await
    }

    #[cfg(feature = "postgres")]
    pub async fn all<M, S>(self, session: &mut S) -> Result<Vec<M>, DbError>
    where
        M: Scannable + for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
        S: DBSession,
    {
        let stmt = self.into_statement_with_suffix(" RETURNING *")?;
        session.fetch_all(stmt).await
    }

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

impl FilteredBuilder for UpdateQuery {
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

fn placeholder_at(pos: usize) -> String {
    #[cfg(feature = "postgres")]
    return format!("${}", pos + 1);
    #[cfg(any(feature = "mysql", feature = "sqlite"))]
    return "?".to_string();
}
