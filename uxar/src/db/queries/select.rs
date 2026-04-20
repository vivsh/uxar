use std::borrow::Cow;
use std::collections::HashMap;
use indexmap::IndexMap;

use crate::db::argvalue::ArgValue;
use crate::db::commons::{Arguments, Row};
use crate::db::executor::{DbError, DBSession};
use crate::db::interfaces::{Filterable, Scannable};
use crate::db::placeholders::{has_named_placeholder, resolve_placeholders, Dialect};
use crate::db::scopes::Scope;
use super::{FilteredBuilder, LockMode, Page, QueryError, Statement};

/// Builder for SELECT queries. Constructed via `db::select(table)`.
#[derive(Clone)]
pub struct SelectQuery {
    source: String,
    alias_map: IndexMap<Cow<'static, str>, Cow<'static, str>>,
    filters: Vec<Cow<'static, str>>,
    group_by: Vec<String>,
    having: Vec<Cow<'static, str>>,
    order_by: Vec<(String, bool)>,
    limit: Option<(usize, usize)>,
    select_exprs: IndexMap<String, Scope>,
    lock_mode: Option<LockMode>,
    args: Arguments<'static>,
    named_args: HashMap<String, ArgValue>,
    error: Option<QueryError>,
}

impl SelectQuery {
    pub(crate) fn new(source: &str) -> Self {
        Self {
            source: source.to_string(),
            alias_map: IndexMap::new(),
            filters: Vec::new(),
            group_by: Vec::new(),
            having: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            select_exprs: IndexMap::new(),
            lock_mode: None,
            args: Arguments::default(),
            named_args: HashMap::new(),
            error: super::validate_ident(source).err(),
        }
    }

    pub fn alias(
        mut self,
        prefix: impl Into<Cow<'static, str>>,
        alias: impl Into<Cow<'static, str>>,
    ) -> Self {
        self.alias_map.insert(prefix.into(), alias.into());
        self
    }

    pub fn group_by(mut self, column: &str) -> Self {
        self.group_by.push(column.to_string());
        self
    }

    pub fn having(mut self, condition: impl Into<Cow<'static, str>>) -> Self {
        self.having.push(condition.into());
        self
    }

    /// Add an ORDER BY clause. Earlier calls have higher precedence.
    pub fn order_by(mut self, column: &str, ascending: bool) -> Self {
        self.order_by.push((column.to_string(), ascending));
        self
    }

    /// Pagination: page numbers are 1-indexed.
    pub fn paginate(self, page: usize, per_page: usize) -> Self {
        let offset = (page.max(1) - 1) * per_page;
        self.slice(offset, per_page)
    }

    pub fn slice(mut self, offset: usize, count: usize) -> Self {
        self.limit = Some((offset, count));
        self
    }

    pub fn select_expr(mut self, name: &str, scope: Scope) -> Self {
        for (k, v) in scope.named_args.clone() {
            self.named_args.insert(k, v);
        }
        self.select_exprs.insert(name.to_string(), scope);
        self
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
        filter.apply_filters_select(self)
    }

    /// Lock selected rows for update (Postgres only).
    #[cfg(feature = "postgres")]
    pub fn for_update(mut self) -> Self {
        self.lock_mode = Some(LockMode::Update);
        self
    }

    /// Lock selected rows in share mode (Postgres only).
    #[cfg(feature = "postgres")]
    pub fn for_share(mut self) -> Self {
        self.lock_mode = Some(LockMode::Share);
        self
    }

    // ── internal builders ─────────────────────────────────────────────────────

    fn build_filter_clause(&self) -> String {
        if self.filters.is_empty() {
            return String::new();
        }
        format!(" WHERE {}", self.filters.join(" AND "))
    }

    fn build_group_by_clause(&self) -> String {
        if self.group_by.is_empty() {
            return String::new();
        }
        format!(" GROUP BY {}", self.group_by.join(", "))
    }

    fn build_having_clause(&self) -> String {
        if self.having.is_empty() {
            return String::new();
        }
        format!(" HAVING {}", self.having.join(" AND "))
    }

    fn build_order_by_clause(&self) -> String {
        if self.order_by.is_empty() {
            return String::new();
        }
        let parts: Vec<String> = self
            .order_by
            .iter()
            .map(|(col, asc)| {
                format!("{} {}", col, if *asc { "ASC" } else { "DESC" })
            })
            .collect();
        format!(" ORDER BY {}", parts.join(", "))
    }

    fn build_limit_clause(&self) -> String {
        match self.limit {
            Some((offset, count)) => format!(" LIMIT {} OFFSET {}", count, offset),
            None => String::new(),
        }
    }

    fn build_lock_clause(&self) -> &'static str {
        match self.lock_mode {
            Some(LockMode::Update) => " FOR UPDATE",
            Some(LockMode::Share) => " FOR SHARE",
            None => "",
        }
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

    fn build_select_sql<M: Scannable>(&mut self) -> String {
        let mut aliases = self.alias_map.clone();
        let mut sql = String::from("SELECT ");
        let col_names = M::scan_column_names();
        let mut first = true;
        for col in &col_names {
            if !first {
                sql.push_str(", ");
            }
            first = false;

            if let Some(dot) = col.rfind('.') {
                let pfx = &col[..dot];
                let rest = &col[dot + 1..];
                let len = aliases.len();
                let alias = aliases
                    .entry(pfx.into())
                    .or_insert_with(|| format!("_t{}", len + 1).into());
                sql.push_str(alias);
                sql.push('.');
                sql.push_str(rest);
            } else {
                if let Some(scope) = self.select_exprs.get(col) {
                    sql.push('(');
                    sql.push_str(&scope.expr);
                    sql.push_str(") AS ");
                }
                sql.push_str(col);
            }
        }
        sql.push_str(" FROM ");
        sql.push_str(&self.source);
        sql.push_str(&self.build_filter_clause());
        sql.push_str(&self.build_group_by_clause());
        sql.push_str(&self.build_having_clause());
        sql.push_str(&self.build_order_by_clause());
        sql.push_str(&self.build_limit_clause());
        sql.push_str(self.build_lock_clause());
        sql
    }

    fn build_count_sql(&mut self) -> String {
        let inner = format!(
            "SELECT * FROM {}{}{}{}",
            self.source,
            self.build_filter_clause(),
            self.build_group_by_clause(),
            self.build_having_clause(),
        );
        format!("SELECT COUNT(*) FROM ({}) AS _counter", inner)
    }

    // ── terminal methods ──────────────────────────────────────────────────────

    pub async fn one<M, S>(mut self, session: &mut S) -> Result<M, DbError>
    where
        M: Scannable + for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
        S: DBSession,
    {
        self.limit = Some((self.limit.map_or(0, |(o, _)| o), 1));
        let sql = self.build_select_sql::<M>();
        let (sql, args) = self.resolve(sql)?;
        session.fetch_one(Statement::new(&sql, args)).await
    }

    pub async fn all<M, S>(mut self, session: &mut S) -> Result<Vec<M>, DbError>
    where
        M: Scannable + for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
        S: DBSession,
    {
        let sql = self.build_select_sql::<M>();
        let (sql, args) = self.resolve(sql)?;
        session.fetch_all(Statement::new(&sql, args)).await
    }

    pub async fn first<M, S>(mut self, session: &mut S) -> Result<Option<M>, DbError>
    where
        M: Scannable + for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
        S: DBSession,
    {
        self.limit = Some((self.limit.map_or(0, |(o, _)| o), 1));
        let sql = self.build_select_sql::<M>();
        let (sql, args) = self.resolve(sql)?;
        session.fetch_optional(Statement::new(&sql, args)).await
    }

    pub async fn count<S: DBSession>(mut self, session: &mut S) -> Result<i64, DbError> {
        self.order_by.clear();
        self.limit = None;
        let sql = self.build_count_sql();
        let (sql, args) = self.resolve(sql)?;
        session.fetch_scalar(Statement::new(&sql, args)).await
    }

    pub async fn exists<S: DBSession>(mut self, session: &mut S) -> Result<bool, DbError> {
        self.order_by.clear();
        self.limit = None;
        let inner = format!(
            "SELECT 1 FROM {}{}{}{}",
            self.source,
            self.build_filter_clause(),
            self.build_group_by_clause(),
            self.build_having_clause(),
        );
        let sql = format!("SELECT EXISTS ({})", inner);
        let (sql, args) = self.resolve(sql)?;
        session.fetch_scalar(Statement::new(&sql, args)).await
    }

    /// Fetch a paginated result set along with the total count.
    pub async fn page<M, S>(self, session: &mut S) -> Result<Page<M>, DbError>
    where
        M: Scannable + for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
        S: DBSession,
    {
        let (page_num, per_page) = match self.limit {
            Some((offset, count)) if count > 0 => {
                (offset / count + 1, count)
            }
            _ => (1, usize::MAX),
        };

        let count = self.clone().count(session).await?;
        let items = self.all::<M, S>(session).await?;

        let total_pages = if per_page == usize::MAX || count == 0 {
            1
        } else {
            ((count as usize) + per_page - 1) / per_page
        };

        Ok(Page {
            items,
            total: count,
            page: page_num,
            per_page,
            total_pages,
        })
    }
}

impl FilteredBuilder for SelectQuery {
    fn filter(mut self, cond: impl Into<Cow<'static, str>>) -> Self {
        self.filters.push(cond.into());
        self
    }

    fn bind_dyn(mut self, val: ArgValue) -> Self {
        use sqlx::Arguments as _;
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
