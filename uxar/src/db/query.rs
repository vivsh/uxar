
use sqlx::postgres::{PgArguments, PgRow};
use sqlx::{Arguments};
use crate::db::{ColumnKind, ColumnSpec, DBSession, DbError, Filterable, SchemaInfo, Model};




/// Naive query builder
/// currently strings containing ? are not escaped.
/// This is a basic implementation and may be extended in future.
pub struct Query {
    sql: String,
    order_by: Option<(String, bool)>,
    limit: Option<(i64, i64)>,
    args: PgArguments,
    error: Option<DbError>,
    where_done: bool,
}

impl Query {
    pub fn new() -> Self {
        Self {
            sql: String::new(),
            order_by: None,
            limit: None,
            args: PgArguments::default(),
            error: None,
            where_done: false,
        }
    }

    pub fn bind<T>(mut self, val: T) -> Self
    where
        T: for<'q> sqlx::Encode<'q, sqlx::Postgres> + sqlx::Type<sqlx::Postgres> + Send + 'static,
    {
        if self.error.is_some() {
            return self;
        }
        match self.args.add(val) {
            Ok(()) => {}
            Err(e) => {
                self.error = Some(DbError::Bind(e.to_string()));
            }
        }
        self
    }

    pub fn filter(self, condition: &str) -> Self {
        let mut qs = self;
        if !qs.where_done {
            qs.sql.push_str(" WHERE ");
            qs.where_done = true;
        } else {
            qs.sql.push_str(" AND ");
        }
        qs.sql.push_str(condition);
        qs
    }

    pub fn debug_query(&self) -> String {
        self.build_query()
    }

    fn build_query(&self) -> String {
        let mut query = self.sql.clone();
        // replace ? with $1, $2, etc.

        let mut param_index = 1;
        let mut result = String::new();
        for ch in query.chars() {
            if ch == '?' {
                result.push('$');
                result.push_str(&param_index.to_string());
                param_index += 1;
            } else {
                result.push(ch);
            }
        }
        query = result;

        if let Some((col, asc)) = &self.order_by {
            query.push_str(" ORDER BY ");
            query.push_str(col);
            if *asc {
                query.push_str(" ASC");
            } else {
                query.push_str(" DESC");
            }
        }
        if let Some((offset, count)) = &self.limit {
            query.push_str(&format!(" LIMIT {} OFFSET {}", count, offset));
        }

        query
    }

    pub(crate) fn into_parts(self) -> Result<(String, PgArguments), DbError> {
        let query = self.build_query();
        let args = self.args;
        match self.error {
            Some(e) => Err(e),
            None => Ok((query, args)),
        }
    }

    pub fn order_by(mut self, column: &str, ascending: bool) -> Self {
        self.order_by = Some((column.to_string(), ascending));
        self
    }

    pub fn slice(mut self, offset: i64, count: i64) -> Self {
        self.limit = Some((offset, count));
        self
    }

    pub async fn exec<S: DBSession>(self, session: &mut S) -> Result<u64, DbError> {
        session.execute(self).await
    }

    pub async fn one<S: DBSession, M>(self, session: &mut S) -> Result<M, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, PgRow> + Send + Unpin,
    {
        session.fetch_one(self).await
    }

    pub async fn all<S: DBSession, M>(self, session: &mut S) -> Result<Vec<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, PgRow> + Send + Unpin,
    {
        session.fetch_all(self).await
    }

    pub async fn first<S: DBSession, M>(self, session: &mut S) -> Result<Option<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, PgRow> + Send + Unpin,
    {
        session.fetch_optional(self).await
    }

    pub async fn as_i64<S: DBSession>(self, session: &mut S) -> Result<i64, DbError> {
        session.fetch_scalar(self).await
    }

    pub async fn as_string<S: DBSession>(self, session: &mut S) -> Result<String, DbError> {
        session.fetch_scalar(self).await
    }

    pub async fn as_bool<S: DBSession>(self, session: &mut S) -> Result<bool, DbError> {
        session.fetch_scalar(self).await
    }

    pub fn push(mut self, sql_fragment: &str) -> Self {
        self.sql.push_str(" ");
        self.sql.push_str(sql_fragment);
        self
    }

    pub fn raw(sql: &str) -> Self {
        let mut qs = Query::new();
        qs = qs.push(sql);
        qs
    }

    fn walk_readable_columns(buffer: &mut String, alias: &str, col_specs: &[ColumnSpec]) {
        for (i, col_spec) in col_specs.iter().filter(|c| c.can_select()).enumerate() {
            if i > 0 {
                buffer.push_str(", ");
            }
            match col_spec.kind {
                ColumnKind::Scalar | ColumnKind::Json => {
                    if !alias.is_empty() {
                        buffer.push_str(alias);
                        buffer.push_str(".");
                    }
                    buffer.push_str(col_spec.db_column);
                }
                ColumnKind::Flatten { columns } => {
                    Self::walk_readable_columns(buffer, alias, columns);
                }
                _ => {
                    unimplemented!("Unsupported column kind in readable select");
                }
            }
        }
    }

    pub fn select<M: Model>(mut self) -> Self {
        let source = M::schema_name();
        self = self.select_from::<M>(source, "");
        self
    }

    pub fn insert<M: Model>(mut self, item: &M) -> Self {
        let source = M::schema_name();
        self = self.insert_into::<M>(item, source);
        self
    }

    pub fn update<M: Model>(mut self, item: &M) -> Self {
        let source = M::schema_name();
        self = self.update_into::<M>(item, source);
        self
    }

    pub fn delete<M: Model>(mut self) -> Self {
        let source = M::schema_name();
        self.sql.push_str("DELETE FROM ");
        self.sql.push_str(source);
        self
    }

    pub fn delete_from(mut self, source: &str) -> Self {
        self.sql.push_str("DELETE FROM ");
        self.sql.push_str(source);
        self
    }

    pub fn select_from<T: Model>(mut self, source: &str, alias: &str) -> Self {
        self.sql.push_str("SELECT ");
        Self::walk_readable_columns(&mut self.sql, alias, T::schema_fields());
        self.sql.push_str(" FROM ");
        self.sql.push_str(source);
        if !alias.is_empty() {
            self.sql.push_str(" AS ");
            self.sql.push_str(alias);
        }
        self
    }

    pub fn returning<M: Model>(mut self) -> Self {
        self.sql.push_str(" RETURNING ");
        Self::walk_readable_columns(&mut self.sql, "", M::schema_fields());
        self
    }

    pub fn returning_with<T: Model>(mut self) -> Self {
        self.sql.push_str(" RETURNING ");
        Self::walk_readable_columns(&mut self.sql, "", T::schema_fields());
        self
    }

    fn writable_column_names<T: Model>() -> Vec<&'static str> {
        let mut cols = Vec::new();
        for col_spec in T::schema_fields().iter().filter(|c| c.can_insert() || c.can_update()) {
            cols.push(col_spec.db_column);
        }
        cols
    }

    pub fn update_into<T: Model>(mut self, item: &T, source: &str) -> Self {
        self.sql.push_str("UPDATE ");
        self.sql.push_str(source);
        self.sql.push_str(" SET ");
        for (i, col_name) in Self::writable_column_names::<T>().iter().enumerate() {
            if i > 0 {
                self.sql.push_str(", ");
            }
            self.sql.push_str(col_name);
            self.sql.push_str(" = ?");
        }
        if let Err(err) = item.bind_values(&mut self.args) {
            self.error = Some(err.into());
            return self;
        }
        self
    }

    pub fn insert_into<T: Model>(mut self, item: &T, source: &str) -> Self {
        self.sql.push_str("INSERT INTO ");
        self.sql.push_str(source);
        self.sql.push_str(" (");
        for (i, col_name) in Self::writable_column_names::<T>().iter().enumerate() {
            if i > 0 {
                self.sql.push_str(", ");
            }
            self.sql.push_str(col_name);
        }
        self.sql.push_str(") VALUES (");
        for i in 0..Self::writable_column_names::<T>().len() {
            if i > 0 {
                self.sql.push_str(", ");
            }
            self.sql.push_str("?");
        }
        self.sql.push_str(")");
        if let Err(err) = item.bind_values(&mut self.args) {
            self.error = Some(err.into());
            return self;
        }
        self
    }

    pub fn insert_many_with<T: Model>(mut self, items: &[T], source: &str) -> Self {
        if items.is_empty() {
            return self;
        }
        self.sql.push_str("INSERT INTO ");
        self.sql.push_str(source);
        self.sql.push_str(" (");
        for (i, col_name) in Self::writable_column_names::<T>().iter().enumerate() {
            if i > 0 {
                self.sql.push_str(", ");
            }
            self.sql.push_str(col_name);
        }
        self.sql.push_str(") VALUES ");
        for (j, item) in items.iter().enumerate() {
            if j > 0 {
                self.sql.push_str(", ");
            }
            self.sql.push_str("(");
            for i in 0..Self::writable_column_names::<T>().len() {
                if i > 0 {
                    self.sql.push_str(", ");
                }
                self.sql.push_str("?");
            }
            self.sql.push_str(")");
            if let Err(err) = item.bind_values(&mut self.args) {
                self.error = Some(err.into());
                return self;
            }
        }
        self
    }

    pub fn filter_with<T: Filterable>(mut self, filter: T) -> Self {
        self = filter.filter_query(self);
        self
    }


    pub fn wrap(mut self, prefix: &str, suffix: &str) -> Self {
        let trimmed = self.sql.trim_end();
        let old_sql = trimmed.strip_suffix(";").unwrap_or(trimmed).trim_end().to_string();
        self.sql = String::new();
        self.sql.push_str(prefix);
        self.sql.push_str(" ");
        self.sql.push_str(&old_sql);
        self.sql.push_str(" ");
        self.sql.push_str(suffix);
        self
    }

    pub async fn count<S: DBSession>(mut self, session: &mut S) -> Result<i64, DbError> {
        self.order_by = None;
        self.limit = None;
        self.wrap("SELECT COUNT(*) FROM (", ") AS counter").as_i64(session).await
    }

    pub async fn exists<S: DBSession>(mut self, session: &mut S) -> Result<bool, DbError> {
        self.order_by = None;
        self.limit = None;
        let query = self.wrap("SELECT EXISTS (", ")");
        query.as_bool(session).await
    }

}

