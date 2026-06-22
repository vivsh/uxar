use serde::Serialize;

use crate::{
    Operation, OperationKind,
    tasks::{TaskRecord, TaskStatus},
};

#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionOut {
    pub subject: String,
    pub roles: u64,
    pub role_names: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct OperationOut {
    pub id: uuid::Uuid,
    pub name: String,
    pub kind: OperationKind,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub path: String,
    pub methods: Vec<&'static str>,
    pub tags: Vec<String>,
    pub owner: Option<String>,
    pub hidden: bool,
    pub conf: Option<serde_json::Value>,
    pub args: Vec<crate::callables::ArgSpec>,
    pub returns: Vec<crate::callables::ReturnSpec>,
}

impl From<&Operation> for OperationOut {
    fn from(op: &Operation) -> Self {
        Self {
            id: op.id,
            name: op.name.clone(),
            kind: op.kind.clone(),
            summary: op.summary.clone(),
            description: op.description.clone(),
            path: op.path.clone(),
            methods: op.http_methods(),
            tags: op.tags.iter().map(|tag| tag.to_string()).collect(),
            owner: op.owner.clone(),
            hidden: op.hidden,
            conf: op.conf.clone(),
            args: op.args.clone(),
            returns: op.returns.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TaskOut {
    pub id: uuid::Uuid,
    pub name: String,
    pub status: TaskStatus,
    pub attempts: i32,
    pub priority: i32,
    pub max_attempts: Option<i32>,
    pub identity: Option<String>,
    pub resume_topic: Option<String>,
    pub last_error: Option<String>,
    pub locked_by: Option<String>,
    pub leased_until: Option<chrono::DateTime<chrono::Utc>>,
    pub ready_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<&TaskRecord> for TaskOut {
    fn from(record: &TaskRecord) -> Self {
        Self {
            id: record.id,
            name: record.name.clone(),
            status: record.status,
            attempts: record.attempts,
            priority: record.priority,
            max_attempts: record.max_attempts,
            identity: record.identity.clone(),
            resume_topic: record.resume_topic.clone(),
            last_error: record.last_error.clone(),
            locked_by: record.locked_by.clone(),
            leased_until: record.leased_until,
            ready_at: record.ready_at,
            created_at: record.created_at,
            updated_at: record.updated_at,
            completed_at: record.completed_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TaskDetailOut {
    #[serde(flatten)]
    pub task: TaskOut,
    pub input: Option<serde_json::Value>,
    pub state: Option<serde_json::Value>,
    pub resume_input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
}

impl From<&TaskRecord> for TaskDetailOut {
    fn from(record: &TaskRecord) -> Self {
        Self {
            task: TaskOut::from(record),
            input: parse_json(&record.input),
            state: record.state.as_deref().and_then(parse_json),
            resume_input: record.resume_input.as_deref().and_then(parse_json),
            output: record.output.as_deref().and_then(parse_json),
            result: record.result.as_deref().and_then(parse_json),
        }
    }
}

fn parse_json(value: &str) -> Option<serde_json::Value> {
    serde_json::from_str(value).ok()
}
