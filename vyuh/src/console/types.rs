use schemars::JsonSchema;
use serde::Serialize;

use crate::{
    Operation, OperationKind,
    callables::{ArgPart, ArgSpec, ReturnPart, ReturnSpec, TypeSchema},
    tasks::{TaskRecord, TaskStatus},
};

#[derive(Debug, Serialize, JsonSchema)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SessionOut {
    pub subject: String,
    pub roles: u64,
    pub role_names: Vec<&'static str>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct OperationOut {
    pub id: String,
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
    pub args: Vec<SchemaItem>,
    pub returns: Vec<SchemaItem>,
}

impl From<&Operation> for OperationOut {
    fn from(op: &Operation) -> Self {
        Self {
            id: op.id.to_string(),
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
            args: op.args.iter().map(SchemaItem::from_arg).collect(),
            returns: op.returns.iter().map(SchemaItem::from_return).collect(),
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SchemaItem {
    pub name: String,
    pub location: String,
    pub description: Option<String>,
    pub status_code: Option<u16>,
    pub content_type: Option<String>,
    pub schema: Option<String>,
}

impl SchemaItem {
    fn from_arg(arg: &ArgSpec) -> Self {
        let (location, schema, content_type) = arg_part(&arg.part);
        Self {
            name: arg.name.clone(),
            location,
            description: arg.description.clone(),
            status_code: None,
            content_type,
            schema,
        }
    }

    fn from_return(ret: &ReturnSpec) -> Self {
        let (location, schema, content_type) = return_part(&ret.part);
        Self {
            name: "response".to_string(),
            location,
            description: ret.description.clone(),
            status_code: ret.status_code,
            content_type,
            schema,
        }
    }
}

fn arg_part(part: &ArgPart) -> (String, Option<String>, Option<String>) {
    match part {
        ArgPart::Header(schema) => ("header".into(), schema_json(schema), None),
        ArgPart::Cookie(schema) => ("cookie".into(), schema_json(schema), None),
        ArgPart::Query(schema) => ("query".into(), schema_json(schema), None),
        ArgPart::Path(schema) => ("path".into(), schema_json(schema), None),
        ArgPart::Body(schema, content_type) => (
            "body".into(),
            schema_json(schema),
            Some(content_type.to_string()),
        ),
        ArgPart::Security { scheme, .. } => (format!("security: {scheme}"), None, None),
        ArgPart::Zone => ("zone".into(), None, None),
        ArgPart::Ignore => ("runtime".into(), None, None),
    }
}

fn return_part(part: &ReturnPart) -> (String, Option<String>, Option<String>) {
    match part {
        ReturnPart::Header(schema) => ("header".into(), schema_json(schema), None),
        ReturnPart::Body(schema, content_type) => (
            "body".into(),
            schema_json(schema),
            Some(content_type.to_string()),
        ),
        ReturnPart::Empty => ("empty".into(), None, None),
        ReturnPart::Unknown => ("unknown".into(), None, None),
    }
}

fn schema_json(schema: &TypeSchema) -> Option<String> {
    serde_json::to_string_pretty(schema).ok()
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TaskOut {
    pub id: String,
    pub name: String,
    pub status: TaskStatus,
    pub attempts: i32,
    pub priority: i32,
    pub max_attempts: Option<i32>,
    pub identity: Option<String>,
    pub last_error: Option<String>,
    pub locked_by: Option<String>,
    pub leased_until: Option<String>,
    pub ready_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

impl From<&TaskRecord> for TaskOut {
    fn from(record: &TaskRecord) -> Self {
        Self {
            id: record.id.to_string(),
            name: record.name.clone(),
            status: record.status,
            attempts: record.attempts,
            priority: record.priority,
            max_attempts: record.max_attempts,
            identity: record.identity.clone(),
            last_error: record.last_error.clone(),
            locked_by: record.locked_by.clone(),
            leased_until: record.leased_until.map(|value| value.to_rfc3339()),
            ready_at: record.ready_at.map(|value| value.to_rfc3339()),
            created_at: record.created_at.to_rfc3339(),
            updated_at: record.updated_at.to_rfc3339(),
            completed_at: record.completed_at.map(|value| value.to_rfc3339()),
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
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
