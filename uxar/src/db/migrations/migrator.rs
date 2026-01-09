use std::{
    collections::{BTreeMap},
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::db::{
    ColumnModel, TableModel,
    migrations::{actions::Action, errors::{MigrationError, StoreError}, stores::MigrationStore},
    models::{StrRef},
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Operation {
    pub action: Action,
    pub reverse: Option<Action>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Migration {
    pub(crate) id: Arc<str>,      // globally unique id
    pub(crate) serial: u64,       // local to group; for tie-breaking
    pub(crate) timestamp_ms: u64, // immutable creation time
    pub(crate) group: Arc<str>,   // could be folder or module name
    pub(crate) parents: Vec<Arc<str>>,
    pub(crate) description: Option<String>,
    pub(crate) operations: Vec<Operation>,
}

pub struct Migrator<S: MigrationStore> {
    pub migrations: Vec<Migration>,
    pub group_serials: BTreeMap<Arc<str>, u64>,
    pub store: S,
}

impl<S: MigrationStore> Migrator<S> {

    pub fn from_migrations<M: IntoIterator<Item = Migration>>(store: S, migrations: M) -> Self {
        let iter = migrations.into_iter();
        let (lower, _) = iter.size_hint();
        let mut migrations_vec = Vec::with_capacity(lower);
        let mut group_serials = BTreeMap::new();
        
        for m in iter {
            let entry = group_serials.entry(m.group.clone()).or_insert(0);
            if m.serial > *entry {
                *entry = m.serial;
            }
            migrations_vec.push(m);
        }
        
        Self {
            migrations: migrations_vec,
            group_serials,
            store,
        }
    }

    pub fn create_migration(
        &mut self,
        group: &str,
        description: Option<String>,
        operations: Vec<Operation>,
    ) -> Migration {
        let group_arc: Arc<str> = Arc::from(group);
        let serial = {
            let entry = self.group_serials.entry(group_arc.clone()).or_insert(0);
            *entry += 1;
            *entry
        };
        let timestamp_ms = chrono::Utc::now().timestamp_millis() as u64;
        
        // Generate descriptive name from operations, Django-style
        let name_slug = super::actions::generate_migration_name(&operations);
        let id = Arc::from(format!("{}-{:04}-{}", group, serial, name_slug));
        
        Migration {
            id,
            serial,
            timestamp_ms,
            group: group_arc,
            parents: Vec::new(),
            description,
            operations,
        }
    }


    pub async fn run(&mut self) -> Result<(), MigrationError> {
        self.run_until(None).await
    }

    async fn run_migrations(store: &S, plan : impl Iterator<Item=&Migration>, forward: bool) -> Result<(), MigrationError> {
         for migration in plan {
            store
                .begin(&migration.id)
                .await
                .map_err(|e| StoreError::begin(&migration.id, e))?;

            for operation in &migration.operations {
                let action = if forward {
                    &operation.action
                } else {
                    match &operation.reverse {
                        Some(rev) => rev,
                        None => {
                            return Err(MigrationError::OperationFailed {
                                migration_id: migration.id.to_string(),
                                detail: "No reverse operation defined".to_string(),
                            });
                        }
                    }
                };
                if let Err(err) = store.run_action(&migration.id, action).await {
                    if let Err(rollback_err) = store.rollback(&migration.id).await {
                        return Err(StoreError::rollback(&migration.id, format!(
                            "original error: {}; rollback error: {}",
                            err, rollback_err
                        )).into());
                    }
                    return Err(MigrationError::OperationFailed {
                        migration_id: migration.id.to_string(),
                        detail: err.to_string(),
                    });
                }
            }

            store.commit(&migration.id, forward).await.map_err(|err| {
                StoreError::commit(&migration.id, err)
            })?;
        }
        Ok(())
    }

    /// Runs migrations to reach a specific target state.
    /// 
    /// If mark is None, applies all pending migrations.
    /// If mark is Some(id), migrates forward or backward to that migration.
    /// 
    /// Strategy:
    /// - Validates applied migrations match the plan prefix (replay check)
    /// - If mark_index < applied_list.len(): rolls back migrations in reverse order
    /// - If mark_index >= applied_list.len(): applies forward migrations in order
    /// 
    /// Each migration runs in its own transaction via the store.
    pub async fn run_until(&mut self, mark: Option<&str>) -> Result<(), MigrationError> {
        let applied_list: Vec<String> = self
            .store
            .get_ordered_applied_id_list()
            .await?
            .into_iter()
            .collect();

        let planner = super::planner::MigPlanner::new();
        let full_plan = planner.plan(&self.migrations)?;

        // Validate that applied migrations match the plan prefix (replay check)
        for (i, applied_id) in applied_list.iter().enumerate() {
            if full_plan.get(i).map(|m| m.id.as_ref()) != Some(&applied_id) {
                return Err(MigrationError::ReplayError(format!(
                    "Mismatch at position {}: expected {}, found {}",
                    i,
                    full_plan.get(i).map(|m| m.id.as_ref()).unwrap_or("none"),
                    applied_id,
                )));
            }
        }

        let mark_index = if let Some(mark_id) = mark {
            let pos = full_plan
                .iter()
                .position(|m| m.id.as_ref() == mark_id)
                .ok_or_else(|| MigrationError::InvalidMigration(format!("Mark id '{}' not found in migrations", mark_id)))?;
            pos + 1  // Convert position to count (mark is inclusive)
        } else {
            full_plan.len()
        };

        if mark_index < applied_list.len() {
            // Rollback scenario: unapply migrations from applied_list.len()-1 down to mark_index
            // Example: applied=[m1,m2,m3] (3 items), mark_index=2 (want m1,m2) → rollback m3
            let count = applied_list.len() - mark_index;
            let mut plan_backward = Vec::with_capacity(count);
            plan_backward.extend(
                full_plan
                    .into_iter()
                    .take(applied_list.len())
                    .skip(mark_index)
                    .rev()
            );
            Self::run_migrations(&self.store, plan_backward.into_iter(), false).await?;
        } else {
            // Forward scenario: apply migrations from applied_list.len() to mark_index-1
            // Example: applied=[m1,m2] (2 items), mark_index=4 (want 4 items) → apply [m3,m4]
            let count = mark_index - applied_list.len();
            let mut plan_forward = Vec::with_capacity(count);
            plan_forward.extend(
                full_plan
                    .into_iter()
                    .skip(applied_list.len())
                    .take(count)
            );
            Self::run_migrations(&self.store, plan_forward.into_iter(), true).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::db::models::QualifiedName;

    use super::*;
    use std::{sync::Mutex, future::Future};

    #[derive(Debug, Clone)]
    struct MockStoreCall {
        method: String,
        migration_id: String,
        forward: Option<bool>,
    }

    struct MockStore {
        applied: Mutex<Vec<String>>,
        calls: Mutex<Vec<MockStoreCall>>,
        fail_on_begin: Option<String>,
        fail_on_action: Option<String>,
        fail_on_commit: Option<String>,
        fail_on_rollback: bool,
    }

    impl MockStore {
        fn new() -> Self {
            Self {
                applied: Mutex::new(Vec::new()),
                calls: Mutex::new(Vec::new()),
                fail_on_begin: None,
                fail_on_action: None,
                fail_on_commit: None,
                fail_on_rollback: false,
            }
        }

        fn with_applied(mut self, applied: Vec<String>) -> Self {
            self.applied = Mutex::new(applied);
            self
        }

        fn fail_on_begin(mut self, id: &str) -> Self {
            self.fail_on_begin = Some(id.to_string());
            self
        }

        fn fail_on_action(mut self, id: &str) -> Self {
            self.fail_on_action = Some(id.to_string());
            self
        }

        fn fail_on_commit(mut self, id: &str) -> Self {
            self.fail_on_commit = Some(id.to_string());
            self
        }

        fn fail_on_rollback(mut self) -> Self {
            self.fail_on_rollback = true;
            self
        }

        fn get_calls(&self) -> Vec<MockStoreCall> {
            self.calls.lock().unwrap().clone()
        }

        fn get_applied(&self) -> Vec<String> {
            self.applied.lock().unwrap().clone()
        }
    }

    impl MigrationStore for MockStore {
        fn get_ordered_applied_id_list(&self) -> impl Future<Output = Result<Vec<String>, MigrationError>> + '_ + Send {
            async move {
                Ok(self.applied.lock().unwrap().clone())
            }
        }

        fn begin(&self, migration_id: &str) -> impl Future<Output = Result<(), MigrationError>> + '_ + Send {
            let migration_id = migration_id.to_string();
            async move {
                self.calls.lock().unwrap().push(MockStoreCall {
                    method: "begin".to_string(),
                    migration_id: migration_id.clone(),
                    forward: None,
                });
                if let Some(ref fail_id) = self.fail_on_begin {
                    if fail_id == &migration_id {
                        return Err(MigrationError::OperationFailed {
                            migration_id,
                            detail: "begin failed".to_string(),
                        });
                    }
                }
                Ok(())
            }
        }

        fn commit(&self, migration_id: &str, forward: bool) -> impl Future<Output = Result<(), MigrationError>> + '_ + Send {
            let migration_id = migration_id.to_string();
            async move {
                self.calls.lock().unwrap().push(MockStoreCall {
                    method: "commit".to_string(),
                    migration_id: migration_id.clone(),
                    forward: Some(forward),
                });
                if let Some(ref fail_id) = self.fail_on_commit {
                    if fail_id == &migration_id {
                        return Err(MigrationError::OperationFailed {
                            migration_id,
                            detail: "commit failed".to_string(),
                        });
                    }
                }
                let mut applied = self.applied.lock().unwrap();
                if forward {
                    applied.push(migration_id);
                } else {
                    applied.retain(|id| id != &migration_id);
                }
                Ok(())
            }
        }

        fn rollback(&self, migration_id: &str) -> impl Future<Output = Result<(), MigrationError>> + '_ + Send {
            let migration_id = migration_id.to_string();
            async move {
                self.calls.lock().unwrap().push(MockStoreCall {
                    method: "rollback".to_string(),
                    migration_id: migration_id.clone(),
                    forward: None,
                });
                if self.fail_on_rollback {
                    return Err(MigrationError::OperationFailed {
                        migration_id,
                        detail: "rollback failed".to_string(),
                    });
                }
                Ok(())
            }
        }

        fn run_action(&self, migration_id: &str, _action: &Action) -> impl Future<Output = Result<(), MigrationError>> + '_ + Send {
            let migration_id = migration_id.to_string();
            async move {
                if let Some(ref fail_id) = self.fail_on_action {
                    if fail_id == &migration_id {
                        return Err(MigrationError::OperationFailed {
                            migration_id,
                            detail: "action failed".to_string(),
                        });
                    }
                }
                Ok(())
            }
        }
    }

    fn mk_migration(id: &str, group: &str, serial: u64, parents: Vec<&str>) -> Migration {
        Migration {
            id: Arc::from(id),
            serial,
            timestamp_ms: 1000 + serial,
            group: Arc::from(group),
            parents: parents.into_iter().map(Arc::from).collect(),
            description: None,
            operations: vec![Operation {
                action: Action::Statement { sql: Arc::from("SELECT 1") },
                reverse: Some(Action::Statement { sql: Arc::from("SELECT 2") }),
            }],
        }
    }

    fn mk_migration_no_reverse(id: &str, group: &str, serial: u64, parents: Vec<&str>) -> Migration {
        Migration {
            id: Arc::from(id),
            serial,
            timestamp_ms: 1000 + serial,
            group: Arc::from(group),
            parents: parents.into_iter().map(Arc::from).collect(),
            description: None,
            operations: vec![Operation {
                action: Action::Statement { sql: Arc::from("SELECT 1") },
                reverse: None,
            }],
        }
    }

    // from_migrations tests

    #[test]
    fn test_from_migrations_empty() {
        let store = MockStore::new();
        let migrator = Migrator::from_migrations(store, vec![]);
        assert_eq!(migrator.migrations.len(), 0);
        assert_eq!(migrator.group_serials.len(), 0);
    }

    #[test]
    fn test_from_migrations_single() {
        let store = MockStore::new();
        let m = mk_migration("m1", "app", 1, vec![]);
        let migrator = Migrator::from_migrations(store, vec![m]);
        assert_eq!(migrator.migrations.len(), 1);
        assert_eq!(migrator.group_serials.get("app"), Some(&1));
    }

    #[test]
    fn test_from_migrations_same_group_max_serial() {
        let store = MockStore::new();
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let m2 = mk_migration("m2", "app", 3, vec![]);
        let m3 = mk_migration("m3", "app", 2, vec![]);
        let migrator = Migrator::from_migrations(store, vec![m1, m2, m3]);
        assert_eq!(migrator.migrations.len(), 3);
        assert_eq!(migrator.group_serials.get("app"), Some(&3));
    }

    #[test]
    fn test_from_migrations_different_groups() {
        let store = MockStore::new();
        let m1 = mk_migration("m1", "app", 2, vec![]);
        let m2 = mk_migration("m2", "auth", 5, vec![]);
        let m3 = mk_migration("m3", "core", 1, vec![]);
        let migrator = Migrator::from_migrations(store, vec![m1, m2, m3]);
        assert_eq!(migrator.migrations.len(), 3);
        assert_eq!(migrator.group_serials.get("app"), Some(&2));
        assert_eq!(migrator.group_serials.get("auth"), Some(&5));
        assert_eq!(migrator.group_serials.get("core"), Some(&1));
    }

    #[test]
    fn test_from_migrations_out_of_order() {
        let store = MockStore::new();
        let m1 = mk_migration("m1", "app", 5, vec![]);
        let m2 = mk_migration("m2", "app", 2, vec![]);
        let m3 = mk_migration("m3", "app", 8, vec![]);
        let migrator = Migrator::from_migrations(store, vec![m1, m2, m3]);
        assert_eq!(migrator.group_serials.get("app"), Some(&8));
    }

    #[test]
    fn test_from_migrations_duplicate_serials() {
        let store = MockStore::new();
        let m1 = mk_migration("m1", "app", 3, vec![]);
        let m2 = mk_migration("m2", "app", 3, vec![]);
        let migrator = Migrator::from_migrations(store, vec![m1, m2]);
        assert_eq!(migrator.migrations.len(), 2);
        assert_eq!(migrator.group_serials.get("app"), Some(&3));
    }

    // create_migration tests

    #[test]
    fn test_create_migration_first_in_group() {
        let store = MockStore::new();
        let mut migrator = Migrator::from_migrations(store, vec![]);
        let m = migrator.create_migration("app", None, vec![]);
        assert_eq!(m.serial, 1);
        assert_eq!(m.group.as_ref(), "app");
        assert!(m.id.starts_with("app-0001-"));
    }

    #[test]
    fn test_create_migration_increments_serial() {
        let store = MockStore::new();
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let mut migrator = Migrator::from_migrations(store, vec![m1]);
        let m2 = migrator.create_migration("app", None, vec![]);
        assert_eq!(m2.serial, 2);
    }

    #[test]
    fn test_create_migration_id_format() {
        let store = MockStore::new();
        let mut migrator = Migrator::from_migrations(store, vec![]);
        let m = migrator.create_migration("myapp", Some("test".to_string()), vec![]);
        // New format: group-serial-name_slug (e.g., "myapp-0001-empty")
        assert!(m.id.starts_with("myapp-0001-"));
        assert!(m.id.contains("empty")); // Empty operations → "empty"
        assert_eq!(m.description, Some("test".to_string()));
    }

    #[test]
    fn test_create_migration_timestamps_monotonic() {
        let store = MockStore::new();
        let mut migrator = Migrator::from_migrations(store, vec![]);
        let m1 = migrator.create_migration("app", None, vec![]);
        let m2 = migrator.create_migration("app", None, vec![]);
        assert!(m2.timestamp_ms >= m1.timestamp_ms);
    }

    #[test]
    fn test_create_migration_empty_operations() {
        let store = MockStore::new();
        let mut migrator = Migrator::from_migrations(store, vec![]);
        let m = migrator.create_migration("app", None, vec![]);
        assert_eq!(m.operations.len(), 0);
    }

    #[test]
    fn test_create_migration_with_reverse() {
        let store = MockStore::new();
        let mut migrator = Migrator::from_migrations(store, vec![]);
        let ops = vec![Operation {
            action: Action::Statement { sql: Arc::from("CREATE TABLE") },
            reverse: Some(Action::Statement { sql: Arc::from("DROP TABLE") }),
        }];
        let m = migrator.create_migration("app", None, ops);
        assert_eq!(m.operations.len(), 1);
        assert!(m.operations[0].reverse.is_some());
    }

    #[test]
    fn test_django_style_migration_names() {
        let store = MockStore::new();
        let mut migrator = Migrator::from_migrations(store, vec![]);
        
        // Single table creation
        let ops = vec![Operation {
            action: Action::CreateTable {
                model: Arc::new(TableModel {
                    qualified_name: QualifiedName::new(Arc::from("users"), None),
                    columns: vec![],
                }),
            },
            reverse: None,
        }];
        let m = migrator.create_migration("auth", None, ops);
        assert!(m.id.contains("create_users"), "Expected 'create_users' in ID: {}", m.id);
        
        // Add column
        let ops = vec![Operation {
            action: Action::CreateColumn {
                table: QualifiedName::parse("users"),
                model: Arc::new(ColumnModel {
                    name: Arc::from("email"),
                    data_type: Arc::from("VARCHAR"),
                    width: Some(255),
                    is_nullable: false,
                    primary_key: false,
                    unique: false,
                    unique_group: None,
                    indexed: false,
                    index_type: None,
                    default: None,
                    check: None,
                    foreign_key: None,
                }),
            },
            reverse: None,
        }];
        let m = migrator.create_migration("auth", None, ops);
        assert!(m.id.contains("add_users_email"), "Expected 'add_users_email' in ID: {}", m.id);
        
        // Multiple operations → "_and_more"
        let ops = vec![
            Operation {
                action: Action::CreateTable {
                    model: Arc::new(TableModel::new(Arc::from("posts"), None)),
                },
                reverse: None,
            },
            Operation {
                action: Action::CreateColumn {
                    table: QualifiedName::parse("posts"),
                    model: Arc::new(ColumnModel::new(Arc::from("title"), Arc::from("TEXT"))),
                },
                reverse: None,
            },
            Operation {
                action: Action::CreateColumn {
                    table: QualifiedName::parse("posts"),
                    model: Arc::new(ColumnModel::new(Arc::from("content"), Arc::from("TEXT"))),
                },
                reverse: None,
            },
        ];
        let m = migrator.create_migration("blog", None, ops);
        assert!(m.id.contains("create_posts_and_more"), "Expected 'create_posts_and_more' in ID: {}", m.id);
    }

    // run_migrations tests

    #[tokio::test]
    async fn test_run_migrations_empty_plan() {
        let store = MockStore::new();
        let result = Migrator::run_migrations(&store, vec![].into_iter(), true).await;
        assert!(result.is_ok());
        assert_eq!(store.get_calls().len(), 0);
    }

    #[tokio::test]
    async fn test_run_migrations_forward_single() {
        let store = MockStore::new();
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let result = Migrator::run_migrations(&store, vec![&m1].into_iter(), true).await;
        assert!(result.is_ok());
        let calls = store.get_calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].method, "begin");
        assert_eq!(calls[1].method, "commit");
        assert_eq!(calls[1].forward, Some(true));
    }

    #[tokio::test]
    async fn test_run_migrations_forward_multiple() {
        let store = MockStore::new();
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let m2 = mk_migration("m2", "app", 2, vec!["m1"]);
        let result = Migrator::run_migrations(&store, vec![&m1, &m2].into_iter(), true).await;
        assert!(result.is_ok());
        let calls = store.get_calls();
        assert_eq!(calls.len(), 4);
        assert_eq!(calls[0].migration_id, "m1");
        assert_eq!(calls[2].migration_id, "m2");
    }

    #[tokio::test]
    async fn test_run_migrations_backward_with_reverse() {
        let store = MockStore::new();
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let result = Migrator::run_migrations(&store, vec![&m1].into_iter(), false).await;
        assert!(result.is_ok());
        let calls = store.get_calls();
        assert_eq!(calls[1].forward, Some(false));
    }

    #[tokio::test]
    async fn test_run_migrations_backward_no_reverse() {
        let store = MockStore::new();
        let m1 = mk_migration_no_reverse("m1", "app", 1, vec![]);
        let result = Migrator::run_migrations(&store, vec![&m1].into_iter(), false).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MigrationError::OperationFailed { migration_id, detail } => {
                assert_eq!(migration_id, "m1");
                assert!(detail.contains("No reverse operation"));
            }
            _ => panic!("Expected OperationFailed"),
        }
    }

    #[tokio::test]
    async fn test_run_migrations_action_error_triggers_rollback() {
        let store = MockStore::new().fail_on_action("m1");
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let result = Migrator::run_migrations(&store, vec![&m1].into_iter(), true).await;
        assert!(result.is_err());
        let calls = store.get_calls();
        assert!(calls.iter().any(|c| c.method == "rollback"));
    }

    #[tokio::test]
    async fn test_run_migrations_rollback_failure() {
        let store = MockStore::new().fail_on_action("m1").fail_on_rollback();
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let result = Migrator::run_migrations(&store, vec![&m1].into_iter(), true).await;
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("rollback"));
    }

    #[tokio::test]
    async fn test_run_migrations_commit_failure() {
        let store = MockStore::new().fail_on_commit("m1");
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let result = Migrator::run_migrations(&store, vec![&m1].into_iter(), true).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_run_migrations_begin_failure() {
        let store = MockStore::new().fail_on_begin("m1");
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let result = Migrator::run_migrations(&store, vec![&m1].into_iter(), true).await;
        assert!(result.is_err());
        let calls = store.get_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method, "begin");
    }

    // run_until tests

    #[tokio::test]
    async fn test_run_until_none_applies_all() {
        let store = MockStore::new();
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let m2 = mk_migration("m2", "app", 2, vec!["m1"]);
        let mut migrator = Migrator::from_migrations(store, vec![m1, m2]);
        let result = migrator.run_until(None).await;
        assert!(result.is_ok());
        let applied = migrator.store.get_applied();
        assert_eq!(applied.len(), 2);
    }

    #[tokio::test]
    async fn test_run_until_some_applied_continues() {
        let store = MockStore::new().with_applied(vec!["m1".to_string()]);
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let m2 = mk_migration("m2", "app", 2, vec!["m1"]);
        let mut migrator = Migrator::from_migrations(store, vec![m1, m2]);
        let result = migrator.run_until(None).await;
        assert!(result.is_ok());
        let applied = migrator.store.get_applied();
        assert_eq!(applied.len(), 2);
        assert_eq!(applied[1], "m2");
    }

    #[tokio::test]
    async fn test_run_until_all_applied_noop() {
        let store = MockStore::new().with_applied(vec!["m1".to_string(), "m2".to_string()]);
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let m2 = mk_migration("m2", "app", 2, vec!["m1"]);
        let mut migrator = Migrator::from_migrations(store, vec![m1, m2]);
        let result = migrator.run_until(None).await;
        assert!(result.is_ok());
        let calls = migrator.store.get_calls();
        assert_eq!(calls.len(), 0);
    }

    #[tokio::test]
    async fn test_run_until_rollback_to_earlier() {
        let store = MockStore::new().with_applied(vec!["m1".to_string(), "m2".to_string(), "m3".to_string()]);
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let m2 = mk_migration("m2", "app", 2, vec!["m1"]);
        let m3 = mk_migration("m3", "app", 3, vec!["m2"]);
        let mut migrator = Migrator::from_migrations(store, vec![m1, m2, m3]);
        let result = migrator.run_until(Some("m2")).await;
        assert!(result.is_ok());
        let calls = migrator.store.get_calls();
        let rollback_calls: Vec<_> = calls.iter().filter(|c| c.forward == Some(false)).collect();
        assert_eq!(rollback_calls.len(), 1);
        assert_eq!(rollback_calls[0].migration_id, "m3");
        let applied = migrator.store.get_applied();
        assert_eq!(applied, vec!["m1", "m2"]);
    }

    #[tokio::test]
    async fn test_run_until_rollback_all() {
        let store = MockStore::new().with_applied(vec!["m1".to_string(), "m2".to_string(), "m3".to_string()]);
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let m2 = mk_migration("m2", "app", 2, vec!["m1"]);
        let m3 = mk_migration("m3", "app", 3, vec!["m2"]);
        let mut migrator = Migrator::from_migrations(store, vec![m1, m2, m3]);
        let result = migrator.run_until(Some("m1")).await;
        assert!(result.is_ok());
        let calls = migrator.store.get_calls();
        let rollback_calls: Vec<_> = calls.iter().filter(|c| c.forward == Some(false)).collect();
        assert_eq!(rollback_calls.len(), 2);
        assert_eq!(rollback_calls[0].migration_id, "m3");
        assert_eq!(rollback_calls[1].migration_id, "m2");
        let applied = migrator.store.get_applied();
        assert_eq!(applied, vec!["m1"]);
    }

    #[tokio::test]
    async fn test_run_until_forward_to_specific() {
        let store = MockStore::new().with_applied(vec!["m1".to_string()]);
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let m2 = mk_migration("m2", "app", 2, vec!["m1"]);
        let m3 = mk_migration("m3", "app", 3, vec!["m2"]);
        let mut migrator = Migrator::from_migrations(store, vec![m1, m2, m3]);
        let result = migrator.run_until(Some("m2")).await;
        assert!(result.is_ok());
        let applied = migrator.store.get_applied();
        assert_eq!(applied.len(), 2);
        assert_eq!(applied[1], "m2");
    }

    #[tokio::test]
    async fn test_run_until_mark_not_found() {
        let store = MockStore::new();
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let mut migrator = Migrator::from_migrations(store, vec![m1]);
        let result = migrator.run_until(Some("nonexistent")).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MigrationError::InvalidMigration(msg) => {
                assert!(msg.contains("not found"));
            }
            _ => panic!("Expected InvalidMigration"),
        }
    }

    #[tokio::test]
    async fn test_run_until_replay_validation_mismatch() {
        let store = MockStore::new().with_applied(vec!["m2".to_string()]);
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let m2 = mk_migration("m2", "app", 2, vec!["m1"]);
        let mut migrator = Migrator::from_migrations(store, vec![m1, m2]);
        let result = migrator.run_until(None).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MigrationError::ReplayError(msg) => {
                assert!(msg.contains("Mismatch"));
            }
            _ => panic!("Expected ReplayError"),
        }
    }

    #[tokio::test]
    async fn test_run_until_replay_validation_different_migration() {
        let store = MockStore::new().with_applied(vec!["m1".to_string(), "m3".to_string()]);
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let m2 = mk_migration("m2", "app", 2, vec!["m1"]);
        let mut migrator = Migrator::from_migrations(store, vec![m1, m2]);
        let result = migrator.run_until(None).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MigrationError::ReplayError(msg) => {
                assert!(msg.contains("position 1"));
            }
            _ => panic!("Expected ReplayError"),
        }
    }

    #[tokio::test]
    async fn test_run_until_mark_equals_current_state() {
        let store = MockStore::new().with_applied(vec!["m1".to_string(), "m2".to_string()]);
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let m2 = mk_migration("m2", "app", 2, vec!["m1"]);
        let m3 = mk_migration("m3", "app", 3, vec!["m2"]);
        let mut migrator = Migrator::from_migrations(store, vec![m1, m2, m3]);
        let result = migrator.run_until(Some("m2")).await;
        assert!(result.is_ok());
        let calls = migrator.store.get_calls();
        assert_eq!(calls.len(), 0);
        let applied = migrator.store.get_applied();
        assert_eq!(applied, vec!["m1", "m2"]);
    }

    // Integration tests

    #[tokio::test]
    async fn test_linear_dependency_chain() {
        let store = MockStore::new();
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let m2 = mk_migration("m2", "app", 2, vec!["m1"]);
        let m3 = mk_migration("m3", "app", 3, vec!["m2"]);
        let mut migrator = Migrator::from_migrations(store, vec![m1, m2, m3]);
        let result = migrator.run_until(None).await;
        assert!(result.is_ok());
        let applied = migrator.store.get_applied();
        assert_eq!(applied, vec!["m1", "m2", "m3"]);
    }

    #[tokio::test]
    async fn test_diamond_dependency() {
        let store = MockStore::new();
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let m2 = mk_migration("m2", "app", 2, vec!["m1"]);
        let m3 = mk_migration("m3", "app", 3, vec!["m1"]);
        let m4 = mk_migration("m4", "app", 4, vec!["m2", "m3"]);
        let mut migrator = Migrator::from_migrations(store, vec![m1, m2, m3, m4]);
        let result = migrator.run_until(None).await;
        assert!(result.is_ok());
        let applied = migrator.store.get_applied();
        assert_eq!(applied.len(), 4);
        let m4_idx = applied.iter().position(|id| id == "m4").unwrap();
        let m2_idx = applied.iter().position(|id| id == "m2").unwrap();
        let m3_idx = applied.iter().position(|id| id == "m3").unwrap();
        assert!(m2_idx < m4_idx);
        assert!(m3_idx < m4_idx);
    }

    #[tokio::test]
    async fn test_multiple_groups_cross_dependencies() {
        let store = MockStore::new();
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let m2 = mk_migration("m2", "auth", 1, vec!["m1"]);
        let m3 = mk_migration("m3", "app", 2, vec!["m2"]);
        let mut migrator = Migrator::from_migrations(store, vec![m1, m2, m3]);
        let result = migrator.run_until(None).await;
        assert!(result.is_ok());
        let applied = migrator.store.get_applied();
        assert_eq!(applied, vec!["m1", "m2", "m3"]);
    }

    #[tokio::test]
    async fn test_rollback_then_forward() {
        let store = MockStore::new().with_applied(vec!["m1".to_string(), "m2".to_string(), "m3".to_string()]);
        let m1 = mk_migration("m1", "app", 1, vec![]);
        let m2 = mk_migration("m2", "app", 2, vec!["m1"]);
        let m3 = mk_migration("m3", "app", 3, vec!["m2"]);
        let mut migrator = Migrator::from_migrations(store, vec![m1, m2, m3]);
        
        let result = migrator.run_until(Some("m2")).await;
        assert!(result.is_ok());
        let applied = migrator.store.get_applied();
        assert_eq!(applied, vec!["m1", "m2"]);
        
        let result = migrator.run_until(Some("m3")).await;
        assert!(result.is_ok());
        let applied = migrator.store.get_applied();
        assert_eq!(applied, vec!["m1", "m2", "m3"]);
    }

    #[tokio::test]
    async fn test_empty_description() {
        let store = MockStore::new();
        let m1 = Migration {
            id: Arc::from("m1"),
            serial: 1,
            timestamp_ms: 1001,
            group: Arc::from("app"),
            parents: vec![],
            description: None,
            operations: vec![],
        };
        let mut migrator = Migrator::from_migrations(store, vec![m1]);
        let result = migrator.run_until(None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_many_operations() {
        let store = MockStore::new();
        let ops: Vec<Operation> = (0..150).map(|i| Operation {
            action: Action::Statement { sql: Arc::from(format!("SELECT {}", i).as_str()) },
            reverse: Some(Action::Statement { sql: Arc::from(format!("SELECT -{}", i).as_str()) }),
        }).collect();
        let m1 = Migration {
            id: Arc::from("m1"),
            serial: 1,
            timestamp_ms: 1001,
            group: Arc::from("app"),
            parents: vec![],
            description: None,
            operations: ops,
        };
        let mut migrator = Migrator::from_migrations(store, vec![m1]);
        let result = migrator.run_until(None).await;
        assert!(result.is_ok());
    }
}
