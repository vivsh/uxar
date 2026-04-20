use crate::db::commons::{Arguments, Row};
use crate::db::executor::{DbError, DBSession};
use crate::db::interfaces::{Bindable, Scannable};
use super::{QueryError, Statement};

/// Builder for INSERT queries. Constructed via `db::insert(table)`.
pub struct InsertQuery {
    source: String,
    args: Arguments<'static>,
    sql: Option<String>,
    error: Option<QueryError>,
}

impl InsertQuery {
    pub(crate) fn new(source: &str) -> Self {
        Self {
            source: source.to_string(),
            args: Arguments::default(),
            sql: None,
            error: None,
        }
    }

    /// Bind a single row for insertion.
    pub fn row<M: Bindable>(mut self, item: &M) -> Self {
        if self.error.is_some() {
            return self;
        }
        let cols = M::bind_column_names();
        if cols.is_empty() {
            self.error = Some(QueryError::BindError("no columns to insert".to_string()));
            return self;
        }
        let mut sql = format!("INSERT INTO {} ({}) VALUES (", self.source, cols.join(", "));
        match self.bind_row_placeholders(item, cols.len(), &mut sql) {
            Ok(()) => {
                sql.push(')');
                self.sql = Some(sql);
            }
            Err(e) => self.error = Some(e),
        }
        self
    }

    /// Bind multiple rows for bulk insertion.
    pub fn rows<M: Bindable>(mut self, items: &[M]) -> Self {
        if self.error.is_some() {
            return self;
        }
        if items.is_empty() {
            self.error = Some(QueryError::BindError(
                "cannot insert empty list".to_string(),
            ));
            return self;
        }
        let cols = M::bind_column_names();
        if cols.is_empty() {
            self.error = Some(QueryError::BindError("no columns to insert".to_string()));
            return self;
        }
        let mut sql = format!("INSERT INTO {} ({}) VALUES ", self.source, cols.join(", "));
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push('(');
            match self.bind_row_placeholders(item, cols.len(), &mut sql) {
                Ok(()) => sql.push(')'),
                Err(e) => {
                    self.error = Some(e);
                    return self;
                }
            }
        }
        self.sql = Some(sql);
        self
    }

    /// INSERT ... ON CONFLICT DO NOTHING (Postgres only).
    #[cfg(feature = "postgres")]
    pub fn upsert<M: Bindable>(mut self, item: &M, conflict_cols: &[&str]) -> Self {
        self = self.row(item);
        if self.error.is_some() {
            return self;
        }
        if let Some(ref mut sql) = self.sql {
            sql.push_str(" ON CONFLICT (");
            sql.push_str(&conflict_cols.join(", "));
            sql.push_str(") DO NOTHING");
        }
        self
    }

    /// INSERT ... ON CONFLICT DO UPDATE SET (Postgres only).
    #[cfg(feature = "postgres")]
    pub fn upsert_update<M: Bindable>(mut self, item: &M, conflict_cols: &[&str]) -> Self {
        self = self.row(item);
        if self.error.is_some() {
            return self;
        }
        let cols = M::bind_column_names();
        let set_clause: Vec<String> = cols
            .iter()
            .filter(|c| !conflict_cols.contains(&c.as_str()))
            .map(|c| format!("{} = EXCLUDED.{}", c, c))
            .collect();
        if let Some(ref mut sql) = self.sql {
            sql.push_str(" ON CONFLICT (");
            sql.push_str(&conflict_cols.join(", "));
            sql.push_str(") DO UPDATE SET ");
            sql.push_str(&set_clause.join(", "));
        }
        self
    }

    // ── internal ──────────────────────────────────────────────────────────────

    fn bind_row_placeholders<M: Bindable>(
        &mut self,
        item: &M,
        col_count: usize,
        sql: &mut String,
    ) -> Result<(), QueryError> {
        use sqlx::Arguments as _;
        let before = self.args.len();
        item.bind_values(&mut self.args)
            .map_err(|e| QueryError::BindError(e.to_string()))?;
        let bound = self.args.len().saturating_sub(before);
        if bound != col_count {
            return Err(QueryError::BindCountMismatch {
                expected: col_count,
                got: bound,
            });
        }
        for i in 0..col_count {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&placeholder_at(before + i));
        }
        Ok(())
    }

    fn into_statement_with_suffix(self, suffix: &str) -> Result<Statement, QueryError> {
        if let Some(err) = self.error {
            return Err(err);
        }
        let base = self.sql.ok_or_else(|| {
            QueryError::BindError("no row data bound — call .row() or .rows() first".to_string())
        })?;
        Ok(Statement::new(&format!("{}{}", base, suffix), self.args))
    }

    // ── terminal methods ──────────────────────────────────────────────────────

    pub async fn execute<S: DBSession>(self, session: &mut S) -> Result<u64, DbError> {
        let stmt = self.into_statement_with_suffix("")?;
        session.execute(stmt).await
    }

    /// Execute and return the first inserted row via RETURNING * (Postgres only).
    #[cfg(feature = "postgres")]
    pub async fn one<M, S>(self, session: &mut S) -> Result<M, DbError>
    where
        M: Scannable + for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
        S: DBSession,
    {
        let stmt = self.into_statement_with_suffix(" RETURNING *")?;
        session.fetch_one(stmt).await
    }

    /// Execute and return all inserted rows via RETURNING * (Postgres only).
    #[cfg(feature = "postgres")]
    pub async fn all<M, S>(self, session: &mut S) -> Result<Vec<M>, DbError>
    where
        M: Scannable + for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
        S: DBSession,
    {
        let stmt = self.into_statement_with_suffix(" RETURNING *")?;
        session.fetch_all(stmt).await
    }

    /// Execute and return the first inserted row if any via RETURNING * (Postgres only).
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

fn placeholder_at(pos: usize) -> String {
    #[cfg(feature = "postgres")]
    return format!("${}", pos + 1);
    #[cfg(any(feature = "mysql", feature = "sqlite"))]
    return "?".to_string();
}
