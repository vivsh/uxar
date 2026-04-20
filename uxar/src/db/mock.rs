
use std::any::Any;
use std::collections::VecDeque;

use crate::db::{DBSession, Database, DbError, Row, Statement};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbCallKind {
    Execute,
    FetchScalar,
    FetchOne,
    FetchAll,
    FetchOptional,
    FetchJsonFirst,
    FetchJsonOne,
    FetchJsonAll,
}

#[derive(Debug)]
pub struct RecordedCall {
    pub kind: DbCallKind,
    pub stmt: Statement,
}

pub struct PlannedCall {
    pub kind: DbCallKind,
    pub sql_contains: Option<&'static str>, // lightweight matcher; improve if needed
    pub response: PlannedResponse,
}

pub enum PlannedResponse {
    OkU64(u64),
    OkString(String),
    OkUnit, // if you ever need
    OkAny(Box<dyn Any + Send + Sync>),       // for fetch_one / scalar
    OkAnyVec(Box<dyn Any + Send + Sync>),    // for fetch_all returning Vec<T>
    OkAnyOpt(Box<dyn Any + Send + Sync>),    // for fetch_optional returning Option<T>
    Err(DbError),
}

pub struct MockDBSession {
    pub recorded: Vec<RecordedCall>,
    planned: VecDeque<PlannedCall>,
    pub strict: bool, // if true: panic on unexpected calls
}

impl MockDBSession {
    pub fn new() -> Self {
        Self {
            recorded: Vec::new(),
            planned: VecDeque::new(),
            strict: true,
        }
    }

    pub fn plan(&mut self, call: PlannedCall) {
        self.planned.push_back(call);
    }

    pub fn plan_execute_ok(&mut self, sql_contains: &'static str, rows: u64) {
        self.plan(PlannedCall {
            kind: DbCallKind::Execute,
            sql_contains: Some(sql_contains),
            response: PlannedResponse::OkU64(rows),
        });
    }

    pub fn plan_fetch_one_ok<T: Send + Sync + 'static>(&mut self, sql_contains: &'static str, value: T) {
        self.plan(PlannedCall {
            kind: DbCallKind::FetchOne,
            sql_contains: Some(sql_contains),
            response: PlannedResponse::OkAny(Box::new(value)),
        });
    }

    pub fn plan_fetch_scalar_ok<T: Send + Sync + 'static>(&mut self, sql_contains: &'static str, value: T) {
        self.plan(PlannedCall {
            kind: DbCallKind::FetchScalar,
            sql_contains: Some(sql_contains),
            response: PlannedResponse::OkAny(Box::new(value)),
        });
    }

    pub fn plan_err(&mut self, kind: DbCallKind, sql_contains: &'static str, err: DbError) {
        self.plan(PlannedCall {
            kind,
            sql_contains: Some(sql_contains),
            response: PlannedResponse::Err(err),
        });
    }

    fn take_next(&mut self, kind: DbCallKind, stmt: &Statement) -> Result<PlannedResponse, DbError> {
        let next = self.planned.pop_front().ok_or_else(|| {
            if self.strict {
                panic!("MockDbSession: unexpected call {:?} with SQL: {:?}", kind, stmt);
            }
            DbError::Unsupported("mock: unexpected call")
        })?;

        if next.kind != kind {
            panic!(
                "MockDbSession: expected call {:?}, got {:?} (SQL: {:?})",
                next.kind, kind, stmt
            );
        }

        if let Some(needle) = next.sql_contains {
            let sql = &stmt.sql;
            if !sql.contains(needle) {
                panic!(
                    "MockDbSession: SQL mismatch. expected contains {:?}, got {:?}",
                    needle, sql
                );
            }
        }

        Ok(next.response)
    }
}


impl DBSession for MockDBSession {
    async fn execute(&mut self, qs: Statement) -> Result<u64, DbError> {
        self.recorded.push(RecordedCall { kind: DbCallKind::Execute, stmt: qs.clone() });
        match self.take_next(DbCallKind::Execute, &qs)? {
            PlannedResponse::OkU64(n) => Ok(n),
            PlannedResponse::Err(e) => Err(e),
            other => panic!("MockDbSession: wrong planned response for execute: {:?}", std::mem::discriminant(&other)),
        }
    }

    async fn fetch_scalar<T>(&mut self, qs: Statement) -> Result<T, DbError>
    where
        for<'d> T: sqlx::Decode<'d, Database> + sqlx::Type<Database> + Send + Unpin + 'static,
    {
        self.recorded.push(RecordedCall { kind: DbCallKind::FetchScalar, stmt: qs.clone() });
        match self.take_next(DbCallKind::FetchScalar, &qs)? {
            PlannedResponse::OkAny(v) => v.downcast::<T>()
                .map(|b| *b)
                .map_err(|_| DbError::Unsupported("mock: fetch_scalar type mismatch")),
            PlannedResponse::Err(e) => Err(e),
            _ => panic!("MockDbSession: wrong planned response for fetch_scalar"),
        }
    }

    async fn fetch_one<M>(&mut self, qs: Statement) -> Result<M, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
    {
        self.recorded.push(RecordedCall { kind: DbCallKind::FetchOne, stmt: qs.clone() });
        match self.take_next(DbCallKind::FetchOne, &qs)? {
            PlannedResponse::OkAny(v) => v.downcast::<M>()
                .map(|b| *b)
                .map_err(|_| DbError::Unsupported("mock: fetch_one type mismatch")),
            PlannedResponse::Err(e) => Err(e),
            _ => panic!("MockDbSession: wrong planned response for fetch_one"),
        }
    }

    async fn fetch_all<M>(&mut self, qs: Statement) -> Result<Vec<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
    {
        self.recorded.push(RecordedCall { kind: DbCallKind::FetchAll, stmt: qs.clone() });
        match self.take_next(DbCallKind::FetchAll, &qs)? {
            PlannedResponse::OkAnyVec(v) => v.downcast::<Vec<M>>()
                .map(|b| *b)
                .map_err(|_| DbError::Unsupported("mock: fetch_all type mismatch")),
            PlannedResponse::Err(e) => Err(e),
            _ => panic!("MockDbSession: wrong planned response for fetch_all"),
        }
    }

    async fn fetch_optional<M>(&mut self, qs: Statement) -> Result<Option<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
    {
        self.recorded.push(RecordedCall { kind: DbCallKind::FetchOptional, stmt: qs.clone() });
        match self.take_next(DbCallKind::FetchOptional, &qs)? {
            PlannedResponse::OkAnyOpt(v) => v.downcast::<Option<M>>()
                .map(|b| *b)
                .map_err(|_| DbError::Unsupported("mock: fetch_optional type mismatch")),
            PlannedResponse::Err(e) => Err(e),
            _ => panic!("MockDbSession: wrong planned response for fetch_optional"),
        }
    }

    async fn fetch_json_first(&mut self, qs: Statement) -> Result<String, DbError> {
        self.recorded.push(RecordedCall { kind: DbCallKind::FetchJsonFirst, stmt: qs.clone() });
        match self.take_next(DbCallKind::FetchJsonFirst, &qs)? {
            PlannedResponse::OkString(s) => Ok(s),
            PlannedResponse::Err(e) => Err(e),
            _ => panic!("MockDbSession: wrong planned response for fetch_json_first"),
        }
    }

    async fn fetch_json_one(&mut self, qs: Statement) -> Result<String, DbError> {
        self.recorded.push(RecordedCall { kind: DbCallKind::FetchJsonOne, stmt: qs.clone() });
        match self.take_next(DbCallKind::FetchJsonOne, &qs)? {
            PlannedResponse::OkString(s) => Ok(s),
            PlannedResponse::Err(e) => Err(e),
            _ => panic!("MockDbSession: wrong planned response for fetch_json_one"),
        }
    }

    async fn fetch_json_all(&mut self, qs: Statement) -> Result<String, DbError> {
        self.recorded.push(RecordedCall { kind: DbCallKind::FetchJsonAll, stmt: qs.clone() });
        match self.take_next(DbCallKind::FetchJsonAll, &qs)? {
            PlannedResponse::OkString(s) => Ok(s),
            PlannedResponse::Err(e) => Err(e),
            _ => panic!("MockDbSession: wrong planned response for fetch_json_all"),
        }
    }
}

/// A dummy pool that can be used in tests without a real database connection.
/// Wraps `MockDBSession` and implements `DBSession` trait.
///
/// # Examples
///
/// ```
/// use uxar::db::mock::DummyPool;
///
/// let mut pool = DummyPool::new();
/// pool.plan_execute_ok("INSERT", 1);
/// // Use pool in your tests
/// ```
pub struct DummyPool {
    session: MockDBSession,
}

impl DummyPool {
    /// Create a new dummy pool with strict mode enabled
    pub fn new() -> Self {
        Self {
            session: MockDBSession::new(),
        }
    }

    /// Create a non-strict dummy pool that returns errors for unexpected calls
    pub fn relaxed() -> Self {
        let mut session = MockDBSession::new();
        session.strict = false;
        Self { session }
    }

    /// Plan an execute call with expected SQL pattern and row count
    pub fn plan_execute_ok(&mut self, sql_contains: &'static str, rows: u64) {
        self.session.plan_execute_ok(sql_contains, rows);
    }

    /// Plan a fetch_one call with expected SQL pattern and return value
    pub fn plan_fetch_one_ok<T: Send + Sync + 'static>(&mut self, sql_contains: &'static str, value: T) {
        self.session.plan_fetch_one_ok(sql_contains, value);
    }

    /// Plan a fetch_scalar call with expected SQL pattern and return value
    pub fn plan_fetch_scalar_ok<T: Send + Sync + 'static>(&mut self, sql_contains: &'static str, value: T) {
        self.session.plan_fetch_scalar_ok(sql_contains, value);
    }

    /// Plan an error response for any call type
    pub fn plan_err(&mut self, kind: DbCallKind, sql_contains: &'static str, err: DbError) {
        self.session.plan_err(kind, sql_contains, err);
    }

    /// Plan a custom call with full control
    pub fn plan(&mut self, call: PlannedCall) {
        self.session.plan(call);
    }

    /// Get access to recorded calls for assertions
    pub fn recorded(&self) -> &[RecordedCall] {
        &self.session.recorded
    }
}

impl Default for DummyPool {
    fn default() -> Self {
        Self::new()
    }
}

impl DBSession for DummyPool {
    async fn execute(&mut self, qs: Statement) -> Result<u64, DbError> {
        self.session.execute(qs).await
    }

    async fn fetch_scalar<T>(&mut self, qs: Statement) -> Result<T, DbError>
    where
        for<'d> T: sqlx::Decode<'d, Database> + sqlx::Type<Database> + Send + Unpin + 'static,
    {
        self.session.fetch_scalar(qs).await
    }

    async fn fetch_one<M>(&mut self, qs: Statement) -> Result<M, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
    {
        self.session.fetch_one(qs).await
    }

    async fn fetch_all<M>(&mut self, qs: Statement) -> Result<Vec<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
    {
        self.session.fetch_all(qs).await
    }

    async fn fetch_optional<M>(&mut self, qs: Statement) -> Result<Option<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static,
    {
        self.session.fetch_optional(qs).await
    }

    async fn fetch_json_first(&mut self, qs: Statement) -> Result<String, DbError> {
        self.session.fetch_json_first(qs).await
    }

    async fn fetch_json_one(&mut self, qs: Statement) -> Result<String, DbError> {
        self.session.fetch_json_one(qs).await
    }

    async fn fetch_json_all(&mut self, qs: Statement) -> Result<String, DbError> {
        self.session.fetch_json_all(qs).await
    }
}

