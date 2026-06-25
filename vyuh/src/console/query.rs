use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    Operation, OperationKind,
    tasks::{TaskListFilter, TaskStatus},
};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct OperationQuery {
    pub kind: Option<String>,
    pub q: Option<String>,
    pub selected: Option<String>,
    pub tag: Option<String>,
    pub owner: Option<String>,
    pub hidden: Option<bool>,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TaskQuery {
    pub status: Option<TaskStatus>,
    pub name: Option<String>,
    pub priority_min: Option<i32>,
    pub identity: Option<String>,
    pub q: Option<String>,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

impl TaskQuery {
    pub fn to_filter(&self, default_limit: usize, max_limit: usize) -> TaskListFilter {
        TaskListFilter {
            status: self.status,
            name: self.name.clone(),
            priority_min: self.priority_min,
            identity: self.identity.clone(),
            q: self.q.clone(),
            limit: clamp_limit(self.limit, default_limit, max_limit),
            offset: parse_cursor(self.cursor.as_deref()),
        }
    }
}

pub fn filter_operations<'a>(
    operations: impl Iterator<Item = &'a Operation>,
    query: &OperationQuery,
    console_bundle_id: Option<uuid::Uuid>,
    default_limit: usize,
    max_limit: usize,
) -> (Vec<&'a Operation>, Option<String>) {
    let offset = parse_cursor(query.cursor.as_deref());
    let limit = clamp_limit(query.limit, default_limit, max_limit);
    let kind = query.kind.as_deref().and_then(parse_kind);
    let q = query.q.as_deref().map(str::to_lowercase);
    let tag = query.tag.as_deref();
    let owner = query.owner.as_deref();

    let mut filtered = operations
        .filter(|op| !is_console_operation(op, console_bundle_id))
        .filter(|op| kind.as_ref().is_none_or(|kind| &op.kind == kind))
        .filter(|op| query.hidden.is_none_or(|hidden| op.hidden == hidden))
        .filter(|op| owner.is_none_or(|owner| op.owner.as_deref() == Some(owner)))
        .filter(|op| tag.is_none_or(|tag| op.tags.iter().any(|candidate| candidate == tag)))
        .filter(|op| {
            q.as_ref().is_none_or(|q| {
                contains(&op.name, q)
                    || op.summary.as_ref().is_some_and(|value| contains(value, q))
                    || op
                        .description
                        .as_ref()
                        .is_some_and(|value| contains(value, q))
                    || contains(&op.path, q)
            })
        })
        .collect::<Vec<_>>();

    filtered.sort_by(|left, right| {
        kind_key(&left.kind)
            .cmp(kind_key(&right.kind))
            .then_with(|| left.name.cmp(&right.name))
    });

    let page = filtered
        .into_iter()
        .skip(offset)
        .take(limit + 1)
        .collect::<Vec<_>>();
    if page.len() > limit {
        (
            page.into_iter().take(limit).collect(),
            Some((offset + limit).to_string()),
        )
    } else {
        (page, None)
    }
}

pub fn is_console_operation(op: &Operation, console_bundle_id: Option<uuid::Uuid>) -> bool {
    console_bundle_id.is_some_and(|bundle_id| op.bundle_id == Some(bundle_id))
}

pub fn clamp_limit(limit: Option<usize>, default_limit: usize, max_limit: usize) -> usize {
    limit.unwrap_or(default_limit).clamp(1, max_limit.max(1))
}

pub fn parse_cursor(cursor: Option<&str>) -> usize {
    cursor
        .and_then(|cursor| cursor.parse::<usize>().ok())
        .unwrap_or(0)
}

fn contains(value: &str, needle_lower: &str) -> bool {
    value.to_lowercase().contains(needle_lower)
}

fn parse_kind(value: &str) -> Option<OperationKind> {
    match value {
        "cron" => Some(OperationKind::Cron),
        "periodic" => Some(OperationKind::Periodic),
        "pgnotify" | "pg_notify" => Some(OperationKind::PgNotify),
        "signal" => Some(OperationKind::Signal),
        "task" => Some(OperationKind::Task),
        "command" => Some(OperationKind::Command),
        "route" => Some(OperationKind::Route),
        "api_doc" | "apidoc" => Some(OperationKind::ApiDoc),
        "service" => Some(OperationKind::Service),
        _ => None,
    }
}

fn kind_key(kind: &OperationKind) -> &'static str {
    match kind {
        OperationKind::Cron => "cron",
        OperationKind::Periodic => "periodic",
        OperationKind::PgNotify => "pgnotify",
        OperationKind::Signal => "signal",
        OperationKind::Task => "task",
        OperationKind::Command => "command",
        OperationKind::Route => "route",
        OperationKind::ApiDoc => "api_doc",
        OperationKind::Service => "service",
    }
}
