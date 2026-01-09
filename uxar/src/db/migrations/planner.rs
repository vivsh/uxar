use super::migrator::Migration;
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

type NodeId = usize;

type ReadySetKey = (Arc<str>, u64, Arc<str>); // (group, serial, id)

#[derive(Debug, thiserror::Error)]
pub enum PlannerError {
    #[error("dependency not found: migration '{child}' depends on '{dep}'")]
    DependencyNotFound { child: String, dep: String },

    #[error("unreachable migrations, probably due to cycles: {0:?}")]
    UnresolvedMigrations(Vec<String>),

    #[error("duplicate migration id: {0}")]
    DuplicateMigration(String),

    #[error("duplicate dependency '{dep}' referenced by migration '{id}'")]
    DuplicateDependency { id: String, dep: String },

    #[error("multiple leaf migrations in group '{group}': {leaves:?}. A merge migration is required.")]
    MultipleHeads { group: String, leaves: Vec<String> },

    #[error("internal planner error: {0}")]
    InternalError(String),

    #[error("invalid node index: {0}")]
    InvalidNodeIndex(usize),
}

pub struct MigNode<'a> {
    index: NodeId,
    children: Vec<NodeId>,
    dependencies: usize,
    migration: &'a Migration,
}

pub struct MigPlanner<'a> {
    arena: Vec<MigNode<'a>>,
}

impl<'a> MigPlanner<'a> {
    pub fn new() -> Self {
        Self { arena: Vec::new() }
    }

    fn insert(&mut self, migration: &'a Migration) -> NodeId {
        let index = self.arena.len();
        self.arena.push(MigNode {
            index,
            dependencies: 0,
            children: Vec::new(),
            migration,
        });
        index
    }

    #[inline]
    fn get_arena_node<'b>(arena: &'b [MigNode<'a>], index: NodeId) -> Result<&'b MigNode<'a>, PlannerError> {
        arena
            .get(index)
            .ok_or_else(|| PlannerError::InvalidNodeIndex(index))
    }

    #[inline]
    fn get_arena_node_mut<'b>(
        arena: &'b mut [MigNode<'a>],
        index: NodeId,
    ) -> Result<&'b mut MigNode<'a>, PlannerError> {
        arena
            .get_mut(index)
            .ok_or_else(|| PlannerError::InvalidNodeIndex(index))
    }

    fn check_multiple_heads(arena: &[MigNode<'a>]) -> Result<(), PlannerError> {
        let mut heads_by_group: HashMap<Arc<str>, Vec<String>> = HashMap::new();

        // Find all leaf nodes (nodes with no children)
        for node in arena {
            if node.children.is_empty() {
                heads_by_group
                    .entry(node.migration.group.clone())
                    .or_default()
                    .push(node.migration.id.to_string());
            }
        }

        // Check if any group has multiple heads
        for (group, leaves) in heads_by_group {
            if leaves.len() > 1 {
                return Err(PlannerError::MultipleHeads {
                    group: group.to_string(),
                    leaves,
                });
            }
        }

        Ok(())
    }

    #[inline]
    fn insert_into_ready_set(
        node: &MigNode<'a>,
        ready_set: &mut BTreeSet<(ReadySetKey, NodeId)>,
    ) -> Result<(), PlannerError> {
        if node.dependencies == 0 {
            let key = (
                node.migration.group.clone(),
                node.migration.serial,
                node.migration.id.clone(),
            );
            ready_set.insert((key, node.index));
        }
        Ok(())
    }

    // Kahn's algorithm: topological sort with cycle detection
    pub(crate) fn plan(
        mut self,
        migrations: &'a [Migration],
    ) -> Result<Vec<&'a Migration>, PlannerError> {
        if migrations.is_empty() {
            return Ok(Vec::new());
        }

        let mut result = Vec::with_capacity(migrations.len());
        let mut id_to_index: HashMap<Arc<str>, NodeId> = HashMap::with_capacity(migrations.len());
        let mut ready: BTreeSet<(ReadySetKey, NodeId)> = BTreeSet::new();
        let mut seen_deps = BTreeSet::new();

        // Build arena and detect duplicate migration IDs
        for m in migrations {
            if id_to_index.contains_key(&m.id) {
                return Err(PlannerError::DuplicateMigration(m.id.to_string()));
            }
            let idx = self.insert(m);
            id_to_index.insert(m.id.clone(), idx);
        }

        // Build dependency graph
        for node_index in 0..self.arena.len() {
            seen_deps.clear();

            let parents_len = Self::get_arena_node(&self.arena, node_index)?.migration.parents.len();
            for parent_idx in 0..parents_len {
                let node = Self::get_arena_node(&self.arena, node_index)?;
                let parent_id = &node.migration.parents[parent_idx];

                if !seen_deps.insert(parent_id) {
                    return Err(PlannerError::DuplicateDependency {
                        id: node.migration.id.to_string(),
                        dep: parent_id.to_string(),
                    });
                }

                let child_id = node.migration.id.clone();
                let dep_id = parent_id.clone();
                let &parent_node_idx = id_to_index.get(parent_id).ok_or_else(|| {
                    PlannerError::DependencyNotFound {
                        child: child_id.to_string(),
                        dep: dep_id.to_string(),
                    }
                })?;

                Self::get_arena_node_mut(&mut self.arena, node_index)?.dependencies += 1;
                Self::get_arena_node_mut(&mut self.arena, parent_node_idx)?.children.push(node_index);
            }

            Self::insert_into_ready_set(Self::get_arena_node(&self.arena, node_index)?, &mut ready)?;
        }

        // Check for multiple heads per group (merge required)
        Self::check_multiple_heads(&self.arena)?;

        // Process migrations in topological order
        while let Some((_, node_index)) = ready.pop_first() {
            let node = Self::get_arena_node(&self.arena, node_index)?;
            result.push(node.migration);

            let children_len = node.children.len();
            for child_idx in 0..children_len {
                let child_index = Self::get_arena_node(&self.arena, node_index)?.children[child_idx];

                let child_node = Self::get_arena_node(&self.arena, child_index)?;
                if child_node.dependencies == 0 {
                    return Err(PlannerError::InternalError(format!(
                        "migration '{}' has zero dependencies but is a child",
                        child_node.migration.id
                    )));
                }

                Self::get_arena_node_mut(&mut self.arena, child_index)?.dependencies -= 1;
                Self::insert_into_ready_set(Self::get_arena_node(&self.arena, child_index)?, &mut ready)?;
            }
        }

        // Detect cycles: if not all migrations were processed, there's a cycle
        if result.len() != self.arena.len() {
            let stuck: Vec<String> = self
                .arena
                .iter()
                .filter(|n| n.dependencies > 0)
                .map(|n| n.migration.id.to_string())
                .collect();
            return Err(PlannerError::UnresolvedMigrations(stuck));
        }

        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn mk_migration(
        id: &str,
        group: &str,
        serial: u64,
        parents: Vec<&str>,
    ) -> Migration {
        Migration {
            id: Arc::from(id),
            group: Arc::from(group),
            serial,
            timestamp_ms: 0,
            parents: parents.into_iter().map(Arc::from).collect(),
            description: None,
            operations: Vec::new(),
        }
    }

    #[test]
    fn test_empty_migrations() {
        let planner = MigPlanner::new();
        let result = planner.plan(&[]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_single_migration() {
        let migrations = vec![mk_migration("m1", "app", 1, vec![])];
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_ok());
        let plan = result.unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].id.as_ref(), "m1");
    }

    #[test]
    fn test_linear_chain() {
        let migrations = vec![
            mk_migration("m1", "app", 1, vec![]),
            mk_migration("m2", "app", 2, vec!["m1"]),
            mk_migration("m3", "app", 3, vec!["m2"]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_ok());
        
        let plan = result.unwrap();
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[0].id.as_ref(), "m1");
        assert_eq!(plan[1].id.as_ref(), "m2");
        assert_eq!(plan[2].id.as_ref(), "m3");
    }

    #[test]
    fn test_multiple_independent() {
        // Multiple independent migrations in same group will trigger MultipleHeads
        let migrations = vec![
            mk_migration("m1", "app", 1, vec![]),
            mk_migration("m2", "app", 2, vec![]),
            mk_migration("m3", "app", 3, vec![]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        // This should fail with MultipleHeads since all 3 are leaf nodes
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PlannerError::MultipleHeads { .. }));
    }

    #[test]
    fn test_diamond_dependency() {
        let migrations = vec![
            mk_migration("m1", "app", 1, vec![]),
            mk_migration("m2", "app", 2, vec!["m1"]),
            mk_migration("m3", "app", 3, vec!["m1"]),
            mk_migration("m4", "app", 4, vec!["m2", "m3"]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_ok());
        
        let plan = result.unwrap();
        assert_eq!(plan.len(), 4);
        assert_eq!(plan[0].id.as_ref(), "m1");
        assert_eq!(plan[3].id.as_ref(), "m4");
    }

    #[test]
    fn test_cross_group_dependencies() {
        let migrations = vec![
            mk_migration("app1_m1", "app1", 1, vec![]),
            mk_migration("app2_m1", "app2", 1, vec!["app1_m1"]),
            mk_migration("app1_m2", "app1", 2, vec!["app2_m1"]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_ok());
        
        let plan = result.unwrap();
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[0].id.as_ref(), "app1_m1");
        assert_eq!(plan[1].id.as_ref(), "app2_m1");
        assert_eq!(plan[2].id.as_ref(), "app1_m2");
    }

    #[test]
    fn test_duplicate_migration_id() {
        let migrations = vec![
            mk_migration("m1", "app", 1, vec![]),
            mk_migration("m1", "app", 2, vec![]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PlannerError::DuplicateMigration(_)));
    }

    #[test]
    fn test_duplicate_dependency() {
        let migrations = vec![
            mk_migration("m1", "app", 1, vec![]),
            mk_migration("m2", "app", 2, vec!["m1", "m1"]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PlannerError::DuplicateDependency { .. }));
    }

    #[test]
    fn test_missing_dependency() {
        let migrations = vec![mk_migration("m1", "app", 1, vec!["nonexistent"])];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PlannerError::DependencyNotFound { .. }));
    }

    #[test]
    fn test_simple_cycle() {
        let migrations = vec![
            mk_migration("m1", "app", 1, vec!["m2"]),
            mk_migration("m2", "app", 2, vec!["m1"]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PlannerError::UnresolvedMigrations(_)));
    }

    #[test]
    fn test_complex_cycle() {
        let migrations = vec![
            mk_migration("m1", "app", 1, vec!["m3"]),
            mk_migration("m2", "app", 2, vec!["m1"]),
            mk_migration("m3", "app", 3, vec!["m2"]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_err());
        
        match result.unwrap_err() {
            PlannerError::UnresolvedMigrations(stuck) => {
                assert_eq!(stuck.len(), 3);
            }
            _ => panic!("Expected UnresolvedMigrations error"),
        }
    }

    #[test]
    fn test_self_dependency() {
        let migrations = vec![mk_migration("m1", "app", 1, vec!["m1"])];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PlannerError::UnresolvedMigrations(_)));
    }

    #[test]
    fn test_same_serial_same_group() {
        // Two migrations with same serial in same group - valid but creates multiple heads
        let migrations = vec![
            mk_migration("m1", "app", 1, vec![]),
            mk_migration("m2", "app", 1, vec![]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        // Should fail with MultipleHeads since both are leaf nodes
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PlannerError::MultipleHeads { .. }));
    }

    #[test]
    fn test_same_serial_different_groups() {
        let migrations = vec![
            mk_migration("m1", "app1", 1, vec![]),
            mk_migration("m2", "app2", 1, vec![]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[test]
    fn test_same_serial_one_has_parent() {
        // Multiple heads scenario: m1 and m2 are both leaf nodes
        // m1 has no parent, m2 depends on m0
        // Both have serial 1 (serials can be duplicated)
        // But both are leaf nodes in same group → requires merge migration
        let migrations = vec![
            mk_migration("m0", "app", 0, vec![]),
            mk_migration("m1", "app", 1, vec![]),
            mk_migration("m2", "app", 1, vec!["m0"]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        // Should fail with MultipleHeads: both m1 and m2 are leaf nodes in "app"
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PlannerError::MultipleHeads { .. }));
    }

    #[test]
    fn test_group_ordering() {
        let migrations = vec![
            mk_migration("m1", "zapp", 1, vec![]),
            mk_migration("m2", "aapp", 1, vec![]),
            mk_migration("m3", "mapp", 1, vec![]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_ok());
        
        let plan = result.unwrap();
        assert_eq!(plan[0].group.as_ref(), "aapp");
        assert_eq!(plan[1].group.as_ref(), "mapp");
        assert_eq!(plan[2].group.as_ref(), "zapp");
    }

    #[test]
    fn test_serial_ordering_within_group() {
        // Three migrations in chain - only last one is head
        let migrations = vec![
            mk_migration("m1", "app", 3, vec![]),
            mk_migration("m2", "app", 1, vec!["m1"]),
            mk_migration("m3", "app", 2, vec!["m2"]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_ok());
        
        let plan = result.unwrap();
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[0].serial, 3);
        assert_eq!(plan[1].serial, 1);
        assert_eq!(plan[2].serial, 2);
    }

    #[test]
    fn test_multiple_parents() {
        let migrations = vec![
            mk_migration("m1", "app", 1, vec![]),
            mk_migration("m2", "app", 2, vec![]),
            mk_migration("m3", "app", 3, vec!["m1", "m2"]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_ok());
        
        let plan = result.unwrap();
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[2].id.as_ref(), "m3");
    }

    #[test]
    fn test_complex_multi_group() {
        let migrations = vec![
            mk_migration("m1", "app1", 1, vec![]),
            mk_migration("m2", "app2", 1, vec!["m1"]),
            mk_migration("m3", "app1", 2, vec!["m2"]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_ok());
        
        let plan = result.unwrap();
        assert_eq!(plan[0].id.as_ref(), "m1");
        assert_eq!(plan[1].id.as_ref(), "m2");
        assert_eq!(plan[2].id.as_ref(), "m3");
    }

    #[test]
    fn test_long_linear_chain() {
        let mut migrations = Vec::new();
        migrations.push(mk_migration("m0", "app", 0, vec![]));
        
        for i in 1..100 {
            let prev = format!("m{}", i - 1);
            let id = format!("m{}", i);
            migrations.push(mk_migration(&id, "app", i as u64, vec![prev.as_str()]));
        }
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_ok());
        
        let plan = result.unwrap();
        assert_eq!(plan.len(), 100);
        for (i, mig) in plan.iter().enumerate() {
            assert_eq!(mig.serial, i as u64);
        }
    }

    #[test]
    fn test_invalid_node_index_protection() {
        let migrations = vec![mk_migration("m1", "app", 1, vec![])];
        let planner = MigPlanner::new();
        
        // This should not panic even with invalid access attempts
        let result = planner.plan(&migrations);
        assert!(result.is_ok());
    }

    #[test]
    fn test_multiple_heads_same_group() {
        // Two leaf migrations in the same group - requires merge
        let migrations = vec![
            mk_migration("m1", "app", 1, vec![]),
            mk_migration("m2", "app", 2, vec![]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PlannerError::MultipleHeads { .. }));
    }

    #[test]
    fn test_multiple_heads_different_groups() {
        // Multiple leaf migrations in different groups - this is fine
        let migrations = vec![
            mk_migration("m1", "app1", 1, vec![]),
            mk_migration("m2", "app2", 1, vec![]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_ok());
    }

    #[test]
    fn test_merge_migration_resolves_heads() {
        // Two branches that are merged - single head
        let migrations = vec![
            mk_migration("m1", "app", 1, vec![]),
            mk_migration("m2a", "app", 2, vec!["m1"]),
            mk_migration("m2b", "app", 2, vec!["m1"]),
            mk_migration("m3_merge", "app", 3, vec!["m2a", "m2b"]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 4);
    }

    #[test]
    fn test_unmerged_branches_error() {
        // Two branches from same parent but not merged
        let migrations = vec![
            mk_migration("m1", "app", 1, vec![]),
            mk_migration("m2a", "app", 2, vec!["m1"]),
            mk_migration("m2b", "app", 2, vec!["m1"]),
        ];
        
        let planner = MigPlanner::new();
        let result = planner.plan(&migrations);
        assert!(result.is_err());
        
        match result.unwrap_err() {
            PlannerError::MultipleHeads { group, leaves } => {
                assert_eq!(group, "app");
                assert_eq!(leaves.len(), 2);
            }
            _ => panic!("Expected MultipleHeads error"),
        }
    }
}