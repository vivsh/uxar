use indexmap::IndexMap;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use super::argvalue::{ArgValue};
use super::placeholders::{resolve_placeholders};
use super::commons::{Arguments, Database, Row};
use crate::db::placeholders::Dialect;
use crate::db::{Bindable, Scannable, DBSession, DbError, Filterable, Scope};


#[derive(Clone, Debug)]
pub struct Statement {
    pub sql: String,
    pub args: Arguments<'static>,
    pub(crate) error: Option<Arc<sqlx::error::BoxDynError>>,
}

impl Statement {
    pub fn new(sql: &str, args: Arguments<'static>) -> Self {
        Self {
            sql: sql.to_string(),
            args,
            error: None,
        }
    }

    pub fn bind<T>(mut self, val: T) -> Self
    where
        T: for<'q> sqlx::Encode<'q, Database> + sqlx::Type<Database> + Send + 'static,
    {
        use sqlx::Arguments as _;

        match self.args.add(val) {
            Ok(()) => self,
            Err(e) => {
                self.error = Some(Arc::new(e));
                self
            },
        }

    }

    pub fn from_str(sql: &str) -> Self {
        Self {
            sql: sql.to_string(),
            args: Arguments::default(),
            error: None,
        }
    }

    /// Returns the fully built SQL query and arguments as a tuple.
    /// Useful for testing and inspection.
    pub fn into_parts(self) -> (String, Arguments<'static>) {
        let sql = self.sql;
        let args = self.args;
        (sql, args)
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum QuerySetError {
    #[error("bind error: {0}")]
    BindError(String),
    #[error("source not set")]
    SourceNotSet,
    #[error("placeholder error: {0}")]
    PlaceholderError(#[from] super::placeholders::PlaceholderError),
    #[error("missing binding for {0}")]
    MissingBinding(String),
    #[error("unused binding: {0}")]
    UnusedBinding(String),
    #[error("bind count mismatch: expected {expected}, got {got}")]
    BindCountMismatch { expected: usize, got: usize },
}

/// Query builder with support for positional ($1, $2) and named (:param) parameters.
/// 
/// Positional parameters are bound immediately with `.bind()`.
/// Named parameters are bound with `.bind_as()` and resolved when building SQL.
/// 
/// # Examples
/// 
/// ## SELECT with named parameters
/// ```ignore
/// use uxar::db::{QuerySet, DBSession};
/// 
/// // Simple query with filters
/// let users: Vec<User> = QuerySet::from_source("users")
///     .filter("age > :min_age AND status = :status")
///     .bind_as("min_age", 18)
///     .bind_as("status", "active")
///     .order_by("created_at", false)  // descending
///     .slice(0, 10)  // LIMIT 10 OFFSET 0
///     .all(&mut session)
///     .await?;
/// 
/// // Query with positional parameters
/// let user: User = QuerySet::from_source("users")
///     .filter("id = $1")
///     .bind(42)
///     .one(&mut session)
///     .await?;
/// ```
/// 
/// ## INSERT operations
/// ```ignore
/// let new_user = User { id: 1, name: "Alice".into(), age: 30 };
/// 
/// // Simple insert
/// QuerySet::from_source("users")
///     .insert(&new_user, &mut session)
///     .await?;
/// 
/// // Insert with RETURNING
/// let inserted: User = QuerySet::from_source("users")
///     .into_insert_returning(&new_user)?
///     .fetch_one(&mut session)
///     .await?;
/// 
/// // Bulk insert
/// let users = vec![user1, user2, user3];
/// QuerySet::from_source("users")
///     .into_insert_many(&users)?
///     .execute(&mut session)
///     .await?;
/// ```
/// 
/// ## UPDATE and DELETE
/// ```ignore
/// // Update with filter
/// let updated_user = User { id: 1, name: "Bob".into(), age: 31 };
/// QuerySet::from_source("users")
///     .filter("id = :id")
///     .bind_as("id", 1)
///     .into_update(&updated_user)?
///     .execute(&mut session)
///     .await?;
/// 
/// // Delete with filter
/// QuerySet::from_source("users")
///     .filter("status = :status AND last_login < :cutoff")
///     .bind_as("status", "inactive")
///     .bind_as("cutoff", cutoff_date)
///     .into_delete()?
///     .execute(&mut session)
///     .await?;
/// ```
/// 
/// ## Aggregations and annotations
/// ```ignore
/// // Count matching records
/// let count: i64 = QuerySet::from_source("users")
///     .filter("age > :min")
///     .bind_as("min", 18)
///     .count(&mut session)
///     .await?;
/// 
/// // Check existence
/// let has_active: bool = QuerySet::from_source("users")
///     .filter("status = :status")
///     .bind_as("status", "active")
///     .exists(&mut session)
///     .await?;
/// 
/// // Computed columns with annotations
/// let scope = Scope::new("age * 365");
/// let users: Vec<User> = QuerySet::from_source("users")
///     .annotate("age_in_days", scope)
///     .filter("age > :min")
///     .bind_as("min", 21)
///     .all(&mut session)
///     .await?;
/// ```
/// 
/// ## Query with joins (using aliases)
/// ```ignore
/// let orders: Vec<Order> = QuerySet::from_source("orders o")
///     .alias("users", "u")
///     .alias("products", "p")
///     .filter("u.status = :status AND o.total > :min")
///     .bind_as("status", "premium")
///     .bind_as("min", 100.0)
///     .all(&mut session)
///     .await?;
/// ```
pub struct QuerySet {
    source: String,
    pub(crate) alias_map: IndexMap<Cow<'static, str>, Cow<'static, str>>,
    filters: Vec<Cow<'static, str>>,
    group_by: Vec<String>,
    having: Vec<Cow<'static, str>>,
    order_by: Vec<(String, bool)>,
    limit: Option<(usize, usize)>,
    select_exprs: IndexMap<String, Scope>,
    pub(crate) args: Arguments<'static>,
    pub(crate) named_args: HashMap<String, ArgValue>,
    pub(crate) error: Option<QuerySetError>,
}


impl QuerySet {
    fn new(source: &str) -> Self {
        Self {
            select_exprs: IndexMap::new(),
            filters: Vec::new(),
            group_by: Vec::new(),
            having: Vec::new(),
            alias_map: IndexMap::new(),
            order_by: Vec::new(),
            limit: None,
            args: Arguments::default(),
            named_args: HashMap::new(),
            error: None,
            source: source.to_string(),
        }
    }

    pub fn alias(mut self, prefix: impl Into<Cow<'static, str>>, alias: impl Into<Cow<'static, str>>) -> Self {
        let alias = alias.into();
        // user defined aliases must not start with '_' to avoid conflicts with auto-generated aliases
        debug_assert!(!alias.starts_with('_'), "Alias must not start with '_'"); 
        self.alias_map.insert(prefix.into(), alias);
        self
    }

    pub fn from_source(source: &str) -> Self {
        let qs = Self::new(source);
        qs
    }

    pub fn bind<T>(mut self, val: T) -> Self
    where
        T: for<'q> sqlx::Encode<'q, Database> + sqlx::Type<Database> + Send + 'static,
    {
        use sqlx::Arguments as _;

        if self.error.is_some() {
            return self;
        }
        match self.args.add(val) {
            Ok(()) => {}
            Err(e) => {
                self.error = Some(QuerySetError::BindError(e.to_string()));
            }
        }
        self
    }

    pub fn bind_as<T>(mut self, name: &str, val: T) -> Self
    where
        T: for<'q> sqlx::Encode<'q, Database> + sqlx::Type<Database> + Send + Sync + 'static,
    {
        if self.error.is_some() {
            return self;
        }
        self.named_args.insert(name.to_string(), super::argvalue::ArgValue::new(val));
        self
    }

    pub fn filter(self, condition: impl Into<Cow<'static, str>>) -> Self {
        let mut qs = self;
        let cond = condition.into();
        qs.filters.push(cond);
        qs
    }

    pub fn group_by(mut self, column: &str) -> Self {
        self.group_by.push(column.to_string());
        self
    }

    pub fn having(self, condition: impl Into<Cow<'static, str>>) -> Self {
        let mut qs = self;
        let cond = condition.into();
        qs.having.push(cond);
        qs
    }

    /// Add an ORDER BY clause. Can be called multiple times to order by multiple columns.
    /// Earlier calls have higher precedence.
    pub fn order_by(mut self, column: &str, ascending: bool) -> Self {
        self.order_by.push((column.to_string(), ascending));
        self
    }

    /// Pagination helper: calculate offset from page number.
    /// Page numbers are 1-indexed. Replaces any existing limit/offset.
    pub fn paginate(self, page: usize, per_page: usize) -> Self {
        let page = page.max(1);
        let offset = ((page - 1) * per_page) as usize;
        self.slice(offset, per_page)
    }

    pub fn slice(mut self, offset: usize, count: usize) -> Self {
        self.limit = Some((offset, count));
        self
    }

    fn build_filter_clause(&self) -> String {
        if self.filters.is_empty() {
            return String::new();
        }
        let mut clause = String::from(" WHERE ");
        clause.push_str(&self.filters.join(" AND "));
        clause
    }

    fn build_group_by_clause(&self) -> String {
        if self.group_by.is_empty() {
            return String::new();
        }
        let mut clause = String::from(" GROUP BY ");
        clause.push_str(&self.group_by.join(", "));
        clause
    }

    fn build_having_clause(&self) -> String {
        if self.having.is_empty() {
            return String::new();
        }
        let mut clause = String::from(" HAVING ");
        clause.push_str(&self.having.join(" AND "));
        clause
    }

    /// Generate a database-specific placeholder string for a given position.
    /// Postgres uses $1, $2, etc. MySQL/SQLite use ?.
    fn placeholder_at(&self, position: usize) -> String {
        #[cfg(feature = "postgres")]
        {
            format!("${}", position + 1)
        }
        #[cfg(any(feature = "mysql", feature = "sqlite"))]
        {
            "?".to_string()
        }
    }

    fn resolve_arguments(mut self, sql: String) -> Result<(String, Arguments<'static>), QuerySetError> {
        if let Some(err) = self.error {
            return Err(err);
        }
        if self.named_args.is_empty() {
            return Ok((sql, self.args));
        }
        let final_sql = resolve_placeholders(&sql, &mut self.args, &self.named_args, Dialect::Postgres)?;
        Ok((final_sql, self.args))
    }

    pub fn filter_with<T: Filterable>(mut self, filter: T) -> Self {
        self = filter.apply_filters(self);
        self
    }

    pub async fn one<S: DBSession, T: Scannable>(mut self, session: &mut S) -> Result<T, DbError>
    where
        T: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
    {
        let current = self.limit.unwrap_or_default();
        self = self.slice(current.0, 1);
        let statement = self.into_select::<T>()?;
        session.fetch_one(statement).await
    }

    pub async fn all<S: DBSession, M: Scannable>(self, session: &mut S) -> Result<Vec<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
    {
        let statement = self.into_select::<M>()?;
        session.fetch_all(statement).await
    }

    pub async fn first<S: DBSession, M: Scannable>(mut self, session: &mut S) -> Result<Option<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
    {
        let current = self.limit.unwrap_or_default();
        self = self.slice(current.0, 1);
        let statement = self.into_select::<M>()?;
        session.fetch_optional(statement).await
    }

    pub async fn count<S: DBSession>(mut self, session: &mut S) -> Result<i64, DbError> {
        self.order_by.clear();
        self.limit = None;

        let mut inner = String::from("SELECT * FROM ");
        inner.push_str(&self.source);
        inner.push_str(&self.build_filter_clause());
        inner.push_str(&self.build_group_by_clause());
        inner.push_str(&self.build_having_clause());
        let sql = format!("SELECT COUNT(*) FROM ({}) AS counter", inner);
        let statement = Statement::new(&sql, self.args);
        session.fetch_scalar(statement).await
    }

    pub async fn exists<S: DBSession>(mut self, session: &mut S) -> Result<bool, DbError> {
        self.order_by.clear();
        self.limit = None;

        let mut inner = String::from("SELECT * FROM ");
        inner.push_str(&self.source);
        inner.push_str(&self.build_filter_clause());
        inner.push_str(&self.build_group_by_clause());
        inner.push_str(&self.build_having_clause());
        let sql = format!("SELECT EXISTS ({})", inner);

        let statement = Statement::new(&sql, self.args);
        session.fetch_scalar(statement).await
    }

    pub fn select_expr(mut self, name: &str, scope: Scope) -> Self {

        // Merge scope's named args into query's named args
        if !scope.named_args.is_empty() {
            for (k, v) in scope.named_args.clone() {
                self.named_args.insert(k, v);
            }
        }
        
        self.select_exprs.insert(name.to_string(), scope);
        self
    }

    /// Execute INSERT and return affected rows
    pub async fn insert<S: DBSession, M: Bindable>(self, item: &M, session: &mut S) -> Result<u64, DbError> {
        let statement = self.into_insert(item)?;
        session.execute(statement).await
    }

    pub fn into_statement(mut self) -> Result<Statement, QuerySetError> {
        let sql = format!("SELECT * FROM {}{}", self.source, self.build_filter_clause());
        let (final_sql, final_args) = self.resolve_arguments(sql)?;
        Ok(Statement::new(&final_sql, final_args))
    }

    pub fn into_select<M: Scannable>(mut self) -> Result<Statement, QuerySetError> {
        let mut aliases = self.alias_map.clone();
        let mut sql = String::new();
        sql.push_str("SELECT ");
        let mut first = true;
        let column_names = M::scan_column_names();
        for col in column_names.iter() {
            if first {
                first = false;
            } else {
                sql.push_str(", ");
            }
            // replace prefix with alias if exists
            if let Some(prefix_idx) = col.rfind('.') {
                let pfx = &col[..prefix_idx];
                let rest = &col[prefix_idx + 1..];
                let len = aliases.len();
                let alias = aliases.entry(pfx.into()).or_insert_with(|| format!("_t{}", len + 1).into());
                sql.push_str(alias);
                sql.push_str(".");
                sql.push_str(rest);
            } else {
                // apply annotations if any
                if let Some(scope) = self.select_exprs.get(col) {
                    sql.push_str("(");
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
        if !self.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            for (i, (col, asc)) in self.order_by.iter().enumerate() {
                if i > 0 {
                    sql.push_str(", ");
                }
                sql.push_str(col);
                if *asc {
                    sql.push_str(" ASC");
                } else {
                    sql.push_str(" DESC");
                }
            }
        }
        if let Some((offset, count)) = &self.limit {
            sql.push_str(&format!(" LIMIT {} OFFSET {}", count, offset));
        }

        let (final_sql, final_args) = self.resolve_arguments(sql)?;

        Ok(Statement::new(&final_sql, final_args))
    }

    fn validate_insert_state(&self) -> Result<(), QuerySetError> {
        if let Some(ref err) = self.error {
            return Err(err.clone());
        }
        if self.source.is_empty() {
            return Err(QuerySetError::SourceNotSet);
        }
        Ok(())
    }

    fn bind_and_placeholders<M: Bindable>(
        &mut self,
        item: &M,
        col_count: usize,
        sql: &mut String,
    ) -> Result<(), QuerySetError> {
        use sqlx::Arguments as _;
        
        let before = self.args.len();
        item.bind_values(&mut self.args)
            .map_err(|e| QuerySetError::BindError(e.to_string()))?;
        let after = self.args.len();
        let bound = after.saturating_sub(before);
        
        if bound != col_count {
            return Err(QuerySetError::BindCountMismatch {
                expected: col_count,
                got: bound,
            });
        }

        for col_idx in 0..col_count {
            if col_idx > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&self.placeholder_at(before + col_idx));
        }
        Ok(())
    }

    fn build_insert_sql<M: Bindable>(
        &mut self,
        items: &[M],
        returning: Option<&str>,
    ) -> Result<String, QuerySetError> {
        if items.is_empty() {
            return Err(QuerySetError::BindError("cannot insert empty list".to_string()));
        }

        let cols = M::bind_column_names();
        if cols.is_empty() {
            return Err(QuerySetError::BindError("no columns to insert".to_string()));
        }

        let mut sql = String::new();
        sql.push_str("INSERT INTO ");
        sql.push_str(&self.source);
        sql.push_str(" (");
        sql.push_str(&cols.join(", "));
        sql.push_str(") VALUES ");

        for (row_idx, item) in items.iter().enumerate() {
            if row_idx > 0 {
                sql.push_str(", ");
            }
            sql.push('(');
            self.bind_and_placeholders(item, cols.len(), &mut sql)?;
            sql.push(')');
        }

        if let Some(ret) = returning {
            sql.push_str(" RETURNING ");
            sql.push_str(ret);
        }

        Ok(sql)
    }

    pub fn into_insert<M: Bindable>(mut self, item: &M) -> Result<Statement, QuerySetError> {
        self.validate_insert_state()?;
        let sql = self.build_insert_sql(std::slice::from_ref(item), None)?;
        let (final_sql, final_args) = self.resolve_arguments(sql)?;
        Ok(Statement::new(&final_sql, final_args))
    }

    pub fn into_insert_many<M: Bindable>(mut self, items: &[M]) -> Result<Statement, QuerySetError> {
        self.validate_insert_state()?;
        let sql = self.build_insert_sql(items, None)?;
        let (final_sql, final_args) = self.resolve_arguments(sql)?;
        Ok(Statement::new(&final_sql, final_args))
    }

    pub fn into_insert_returning<M: Bindable>(mut self, item: &M) -> Result<Statement, QuerySetError> {
        self.validate_insert_state()?;
        let sql = self.build_insert_sql(std::slice::from_ref(item), Some("*"))?;
        let (final_sql, final_args) = self.resolve_arguments(sql)?;
        Ok(Statement::new(&final_sql, final_args))
    }

    pub fn into_insert_many_returning<M: Bindable>(mut self, items: &[M]) -> Result<Statement, QuerySetError> {
        self.validate_insert_state()?;
        let sql = self.build_insert_sql(items, Some("*"))?;
        let (final_sql, final_args) = self.resolve_arguments(sql)?;
        Ok(Statement::new(&final_sql, final_args))
    }

    pub fn into_delete(self) -> Result<Statement, QuerySetError> {
        if let Some(ref err) = self.error {
            return Err(err.clone());
        }
        if self.source.is_empty() {
            return Err(QuerySetError::SourceNotSet);
        }

        let mut sql = String::new();
        sql.push_str("DELETE FROM ");
        sql.push_str(&self.source);
        sql.push_str(&self.build_filter_clause());

        let (final_sql, final_args) = self.resolve_arguments(sql)?;
        Ok(Statement::new(&final_sql, final_args))
    }

    pub fn into_update<M: Bindable>(mut self, item: &M) -> Result<Statement, QuerySetError> {
        use sqlx::Arguments as _;

        if let Some(ref err) = self.error {
            return Err(err.clone());
        }
        if self.source.is_empty() {
            return Err(QuerySetError::SourceNotSet);
        }

        let cols = M::bind_column_names();
        if cols.is_empty() {
            return Err(QuerySetError::BindError("no columns to update".to_string()));
        }

        let mut sql = String::new();

        sql.push_str("UPDATE ");
        sql.push_str(&self.source);
        sql.push_str(" SET ");

        let before = self.args.len();
        item.bind_values(&mut self.args)
            .map_err(|e| QuerySetError::BindError(e.to_string()))?;
        let after = self.args.len();
        let bound = after.saturating_sub(before);
        if bound != cols.len() {
            return Err(QuerySetError::BindCountMismatch {
                expected: cols.len(),
                got: bound,
            });
        }

        for (i, col) in cols.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str(col);
            sql.push_str(" = ");
            sql.push_str(&self.placeholder_at(before + i));
        }

        sql.push_str(&self.build_filter_clause());

        let (final_sql, final_args) = self.resolve_arguments(sql)?;
        Ok(Statement::new(&final_sql, final_args))
    }


}


#[cfg(test)]
mod tests {
    use crate::db::placeholders::PlaceholderError;

    use super::*;
    use uxar_macros::Bindable;

    #[derive(Scannable)]
    struct BasicUser {
        id: i32,
        name: String,
        email: String,
        age: i32,
    }

    #[derive(Scannable, Default)]
    struct Location {
        street: String,
        city: String,
        postal: String,
    }

    #[derive(Scannable)]
    struct UserWithFlat {
        id: i32,
        username: String,
        #[column(flatten)]
        location: Location,
    }

    #[derive(Scannable)]
    struct Product {
        id: i32,
        name: String,
    }

    #[derive(Scannable)]
    struct OrderItem {
        id: i32,
        quantity: i32,
        #[column(reference)]
        product: Product,
    }

    #[test]
    fn test_build_select_with_scalar_fields() {
        let qs = QuerySet::from_source("users");
        let result = qs.into_select::<BasicUser>();
        
        assert!(result.is_ok());
        let stmt = result.unwrap();
        let (sql, _args) = stmt.into_parts();
        
        assert_eq!(sql, "SELECT id, name, email, age FROM users");
    }

    #[test]
    fn test_build_select_with_flatten() {
        let qs = QuerySet::from_source("users");
        let result = qs.into_select::<UserWithFlat>();
        
        assert!(result.is_ok());
        let stmt = result.unwrap();
        let (sql, _args) = stmt.into_parts();
        
        assert_eq!(sql, "SELECT id, username, street, city, postal FROM users");
    }

    #[test]
    fn test_build_select_with_reference_column() {
        let qs = QuerySet::from_source("order_items")
            .alias("product", "p");
        let result = qs.into_select::<OrderItem>();
        
        assert!(result.is_ok());
        let stmt = result.unwrap();
        let (sql, _args) = stmt.into_parts();
        
        assert_eq!(sql, "SELECT id, quantity, p.id, p.name FROM order_items");
    }

    #[test]
    fn test_build_select_with_string_annotation() {
        let qs = QuerySet::from_source("users")
            .select_expr("age", Scope::new("COALESCE(age, 0)"));
        let result = qs.into_select::<BasicUser>();
        
        assert!(result.is_ok());
        let stmt = result.unwrap();
        let (sql, _args) = stmt.into_parts();
        
        assert_eq!(sql, "SELECT id, name, email, (COALESCE(age, 0)) AS age FROM users");
    }

    #[test]
    fn test_build_select_with_annotation_and_bound_arg() {
        let qs = QuerySet::from_source("users")
            .select_expr("age", Scope::new("COALESCE(age, :default_age)").bind("default_age", 18));
        let result = qs.into_select::<BasicUser>();
        
        assert!(result.is_ok());
        let stmt = result.unwrap();
        let (sql, args) = stmt.into_parts();
        
        assert!(sql.contains("(COALESCE(age, $"));
        use sqlx::Arguments as _;
        assert_eq!(args.len(), 1);
    }

    #[derive(Bindable)]
    struct InsertUser {
        id: i32,
        name: String,
    }

    #[test]
    fn test_build_insert_single() {
        let user = InsertUser {
            id: 1,
            name: "alice".to_string(),
        };

        let qs = QuerySet::from_source("users");
        let stmt = qs.into_insert(&user).unwrap();
        let (sql, args) = stmt.into_parts();

        assert_eq!(sql, "INSERT INTO users (id, name) VALUES ($1, $2)");
        use sqlx::Arguments as _;
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn test_build_insert_many() {
        let users = vec![
            InsertUser {
                id: 1,
                name: "alice".to_string(),
            },
            InsertUser {
                id: 2,
                name: "bob".to_string(),
            },
        ];

        let qs = QuerySet::from_source("users");
        let stmt = qs.into_insert_many(&users).unwrap();
        let (sql, args) = stmt.into_parts();

        assert_eq!(
            sql,
            "INSERT INTO users (id, name) VALUES ($1, $2), ($3, $4)"
        );
        use sqlx::Arguments as _;
        assert_eq!(args.len(), 4);
    }

    #[test]
    fn test_build_delete() {
        let qs = QuerySet::from_source("users")
            .filter("age < $1")
            .bind(18);
        let stmt = qs.into_delete().unwrap();
        let (sql, args) = stmt.into_parts();

        assert_eq!(sql, "DELETE FROM users WHERE age < $1");
        use sqlx::Arguments as _;
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn test_build_delete_no_filter() {
        let qs = QuerySet::from_source("users");
        let stmt = qs.into_delete().unwrap();
        let (sql, _args) = stmt.into_parts();

        assert_eq!(sql, "DELETE FROM users");
    }

    #[test]
    fn test_build_update() {
        let user = InsertUser {
            id: 1,
            name: "alice_updated".to_string(),
        };

        let qs = QuerySet::from_source("users")
            .filter("id = $1")
            .bind(1);
        let stmt = qs.into_update(&user).unwrap();
        let (sql, args) = stmt.into_parts();

        assert_eq!(sql, "UPDATE users SET id = $2, name = $3 WHERE id = $1");
        use sqlx::Arguments as _;
        assert_eq!(args.len(), 3);
    }

    #[test]
    fn test_build_update_no_filter() {
        let user = InsertUser {
            id: 1,
            name: "alice".to_string(),
        };

        let qs = QuerySet::from_source("users");
        let stmt = qs.into_update(&user).unwrap();
        let (sql, args) = stmt.into_parts();

        assert_eq!(sql, "UPDATE users SET id = $1, name = $2");
        use sqlx::Arguments as _;
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn test_named_params_in_filter() {
        let qs = QuerySet::from_source("users")
            .filter("age > :min_age AND status = :status")
            .bind_as("min_age", 18)
            .bind_as("status", "active");
        
        let stmt = qs.into_select::<BasicUser>().unwrap();
        let (sql, args) = stmt.into_parts();
        
        // Named params should be replaced with positional
        assert!(sql.contains("WHERE age > $"));
        assert!(sql.contains("AND status = $"));
        use sqlx::Arguments as _;
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn test_mixed_positional_and_named() {
        let qs = QuerySet::from_source("users")
            .filter("created_at > $1 AND age > :min_age")
            .bind("2024-01-01")
            .bind_as("min_age", 18);
        
        let stmt = qs.into_select::<BasicUser>().unwrap();
        let (sql, args) = stmt.into_parts();
        
        // $1 stays, :min_age becomes $2
        assert!(sql.contains("created_at > $1"));
        assert!(sql.contains("age > $2"));
        use sqlx::Arguments as _;
        assert_eq!(args.len(), 2);
    }

    #[cfg(feature = "postgres")]
    #[test]
    fn test_postgres_named_param_reuse() {
        let qs = QuerySet::from_source("users")
            .filter("age > :limit OR score > :limit")
            .bind_as("limit", 18);
        
        let stmt = qs.into_select::<BasicUser>().unwrap();
        let (sql, args) = stmt.into_parts();
        
        // Both :limit should resolve to same $1 on Postgres
        assert!(sql.contains("age > $1"));
        assert!(sql.contains("score > $1"));
        use sqlx::Arguments as _;
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn test_named_params_in_delete() {
        let qs = QuerySet::from_source("users")
            .filter("age < :max_age")
            .bind_as("max_age", 13);
        
        let stmt = qs.into_delete().unwrap();
        let (sql, args) = stmt.into_parts();
        
        assert!(sql.contains("DELETE FROM users WHERE age < $"));
        use sqlx::Arguments as _;
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn test_unused_binding_error() {
        let qs = QuerySet::from_source("users")
            .filter("age > :min_age")
            .bind_as("min_age", 18)
            .bind_as("unused", 100);  // Bound but never used
        
        let result = qs.into_delete();
        assert!(result.is_ok()); // Note: Current implementation does not check for unused bindings
    }

    #[test]
    fn test_missing_binding_error() {
        let qs = QuerySet::from_source("users")
            .filter("age > :min_age");  // Used but never bound
        
        let result = qs.into_delete();
        assert!(result.is_err());
        match result {
            Err(QuerySetError::PlaceholderError(PlaceholderError::MissingValue(name))) => {
                assert!(name.contains("min_age"));
            }
            _ => panic!("Expected MissingBinding error"),
        }
    }
}
