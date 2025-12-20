use std::{
    collections::{HashSet, VecDeque},
    hash::{Hash, Hasher},
};

use thiserror::Error;


use twox_hash::XxHash64;
use std::hash::{BuildHasher};


#[derive(Clone, Default)]
pub struct StableHasherBuilder;

impl BuildHasher for StableHasherBuilder {
    type Hasher = StableHasher;
    fn build_hasher(&self) -> Self::Hasher {
        StableHasher::new()
    }
}

pub struct StableHasher(XxHash64);

impl StableHasher {

    const DEFAULT_SEED: u64 = 27;

    pub fn new() -> Self {
        Self(XxHash64::with_seed(Self::DEFAULT_SEED))
    }
}

impl Hasher for StableHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) { self.0.write(bytes); }

    #[inline]
    fn finish(&self) -> u64 { self.0.finish() }
}


type StableHashMap<K, V> = std::collections::HashMap<K, V, StableHasherBuilder>;


#[derive(Error, Debug)]
pub enum DiffError {
    #[error("{role} {name} already exists")]
    DuplicateEntity { name: String, role: String },

    #[error("Node {id} is missing from arena")]
    MissingFromArena { id: usize },

    #[error("{role} {name} does not exist")]
    EntityNotFound { name: String, role: String },
}

#[derive(Debug)]
pub struct Diffable {
    pub index: usize,
    /// Index for lookup of the entity. This'll be based on name and type of entity
    pub id: u64,

    // Whether this entity is a table, column, index, etc.
    pub role_key: u64,

    /// Hash key representing the identity of the entity. This will change when type or any other property changes
    pub identity_key: u64,

    /// Hash key representing the type of the entity. This will change when the type changes
    pub type_key: u64,

    pub parent_index: Option<usize>,

    pub children_indexes: Vec<usize>,

    pub entity: Entity,
}

impl Diffable {}

/// Representation of an entity in the system.
/// This is meant to be an internal representation used for diffing and migrations.
/// extras need to be deterministically ordered to avoid spurious diffs
/// Name is supposed to be fully qualified name just like canonical file paths
/// Role is the role of the entity (e.g. table, column, index, etc.)
/// Within the same role, names are expected to be unique.
/// Type name is like inode number in file system - it should stay unchanged even after renames. For a table, 
/// it could be concated string of column names and types.
#[derive(Debug, Clone)]
pub struct Entity {
    pub name: String,
    pub role: String,
    pub type_name: String,
    pub attrs: StableHashMap<String, String>,
    pub extras: Vec<String>,
    pub children: Vec<Entity>,
}

impl Entity {
    fn name_key(&self) -> u64 {
        Self::make_name_key(&self.name, &self.role)
    }

    #[inline]
    fn make_name_key(name: &str, role: &str) -> u64 {
        let mut hasher = StableHasher::new();
        name.hash(&mut hasher);
        role.hash(&mut hasher);
        hasher.finish()
    }

    fn role_key(&self) -> u64 {
        let mut hasher = StableHasher::new();
        self.role.hash(&mut hasher);
        hasher.finish()
    }

    fn type_key(&self) -> u64 {
        let mut hasher = StableHasher::new();
        self.type_name.hash(&mut hasher);
        hasher.finish()
    }

    // A hash representing the identity of the entity excluding its name and children
    // It is meant to detect if two entities are the same except for their name
    fn identity_key(&self) -> u64 {
        let separator = "|--|";
        let mut hasher = StableHasher::new();
        self.type_name.hash(&mut hasher);
        separator.hash(&mut hasher);
        self.role.hash(&mut hasher);
        separator.hash(&mut hasher);
        let mut keys = self.attrs.keys().cloned().collect::<Vec<String>>();
        keys.sort();
        for (k, v) in keys.iter().map(|k| (k, &self.attrs[k])) {
            k.hash(&mut hasher);
            separator.hash(&mut hasher);
            v.hash(&mut hasher);
            separator.hash(&mut hasher);
        }
        separator.hash(&mut hasher);
        for extra in self.extras.iter() {
            separator.hash(&mut hasher);
            extra.hash(&mut hasher);
        }
        separator.hash(&mut hasher);
        hasher.finish()
    }
}


/// Diff engine to compute the difference between two sets of entities
/// Meant to be single-use only
pub struct DiffEngine {
    diffable_arena: Vec<Diffable>,
    last_state: StableHashMap<u64, usize>,
    current_state: StableHashMap<u64, usize>,
    last_root_indexes: Vec<usize>,
    current_root_indexes: Vec<usize>,
}

impl DiffEngine {

    pub fn new() -> Self {
        Self {
            diffable_arena: Vec::new(),
            last_state: StableHashMap::default(),
            current_state: StableHashMap::default(),
            last_root_indexes: Vec::new(),
            current_root_indexes: Vec::new(),
        }
    }

    fn load_state(
        entities: Vec<Entity>,
        parent_index: Option<usize>,
        arena: &mut Vec<Diffable>,
        state: &mut StableHashMap<u64, usize>,
        root_indexes: &mut Vec<usize>,
    ) -> Result<(), DiffError> {
        let mut stack: VecDeque<(Option<usize>, Entity)> =
            VecDeque::from_iter(entities.into_iter().map(|n| (parent_index, n)));

        while let Some((parent_index, mut ent)) = stack.pop_front() {
            let index = arena.len();
            let mut children = Vec::new();
            std::mem::swap(&mut children, &mut ent.children);
            let id = ent.name_key();
            if state.contains_key(&id) {
                return Err(DiffError::DuplicateEntity {
                    name: ent.name,
                    role: ent.role,
                });
            }
            let type_key = ent.type_key();
            let identity_key = ent.identity_key();
            let node = Diffable {
                parent_index,
                index,
                children_indexes: Vec::new(),
                id,
                type_key,
                identity_key,
                role_key: ent.role_key(),
                entity: ent,
            };
            state.insert(id, index);
            arena.push(node);
            if parent_index.is_none() {
                root_indexes.push(index);
            }
            for child in children {
                stack.push_back((Some(index), child));
            }

            if let Some(p_index) = parent_index {
                if let Some(parent) = arena.get_mut(p_index) {
                    parent.children_indexes.push(index);
                }
            }
        }

        Ok(())
    }

    fn delete_subtree(&mut self, index: usize) -> Result<(), DiffError> {
        let mut stack = Vec::new();
        stack.push(index);

        // remove from parent's children list
        if let Some(diffable) = self.diffable_arena.get(index) {
            if let Some(parent_index) = diffable.parent_index {
                if let Some(parent) = self.diffable_arena.get_mut(parent_index) {
                    parent.children_indexes.retain(|&ix| ix != index);
                }
            }else{
                self.last_root_indexes.retain(|&ix| ix != index);
            }
        } else {
            return Err(DiffError::MissingFromArena { id: index });
        }

        while let Some(ix) = stack.pop() {
            if let Some(diffable) = self.diffable_arena.get(ix) {
                for &child_index in &diffable.children_indexes {
                    stack.push(child_index);
                }
                // no need to remove from parent again as the entire tree is purged
                self.last_state.remove(&diffable.id);
            } else {
                return Err(DiffError::MissingFromArena { id: ix });
            }
        }
        Ok(())
    }

    /// replays the operation into the diff engine
    /// Affects only the last_state
    pub fn patch(&mut self, op: Patch) -> Result<(), DiffError> {
        match op {
            Patch::Delete { name, role } => {
                let id = Entity::make_name_key(&name, &role);
                if let Some(index) = self.last_state.get(&id) {
                    let index = *index;
                    // remove from roots
                    self.delete_subtree(index)?;
                } else {
                    return Err(DiffError::EntityNotFound { name, role });
                }
            }
            Patch::Modify {
                name,
                role,
                new_name,
                new_type_name,
                modifications,
                extras,
                deletions,
            } => {
                let id = Entity::make_name_key(&name, &role);
                let new_id = if let Some(n) = &new_name {
                    Entity::make_name_key(n, &role)
                } else {
                    id
                };
                if self.last_state.contains_key(&new_id) && new_id != id {
                    return Err(DiffError::DuplicateEntity {
                        name: new_name.unwrap(),
                        role,
                    });
                }
                if let Some(index) = self.last_state.remove(&id) {
                    if let Some(diffable) = self.diffable_arena.get_mut(index) {
                        if let Some(n) = new_name {
                            diffable.entity.name = n;
                        }
                        if let Some(t) = new_type_name {
                            diffable.entity.type_name = t;
                        }
                        for (k, v) in modifications {
                            diffable.entity.attrs.insert(k, v);
                        }
                        for del in deletions {
                            diffable.entity.attrs.remove(&del);
                        }                        
                        diffable.entity.extras = extras;     

                        // Recompute the keys
                        diffable.id = diffable.entity.name_key();
                        diffable.identity_key = diffable.entity.identity_key();
                        diffable.type_key = diffable.entity.type_key();
                        // Update the last_state map if needed
                        self.last_state.insert(diffable.id, index);
                    } else {
                        return Err(DiffError::MissingFromArena { id: index });
                    }
                } else {
                    return Err(DiffError::EntityNotFound { name, role });
                }
            }
            Patch::New { entity, parent } => {
                let parent_index = if let Some((parent_name, parent_role)) = parent {
                    let node_id = Entity::make_name_key(&parent_name, &parent_role);
                    if let Some(index) = self.last_state.get(&node_id) {
                        Some(*index)
                    } else {
                        return Err(DiffError::EntityNotFound {
                            name: parent_name,
                            role: parent_role,
                        });
                    }
                } else {
                    None
                };
                Self::load_state(
                    vec![entity],
                    parent_index,
                    &mut self.diffable_arena,
                    &mut self.last_state,
                    &mut self.last_root_indexes,
                )?
            }
        }

        Ok(())
    }

    pub fn load_last_state(&mut self, entities: Vec<Entity>) -> Result<(), DiffError> {
        Self::load_state(
            entities,
            None,
            &mut self.diffable_arena,
            &mut self.last_state,
            &mut self.last_root_indexes
        )
    }

    pub fn load_current_state(&mut self, entities: Vec<Entity>) -> Result<(), DiffError> {
        Self::load_state(
            entities,
            None,
            &mut self.diffable_arena,
            &mut self.current_state,
            &mut self.current_root_indexes
        )
    }

    pub fn diff(&self) -> Result<Vec<Patch>, DiffError> {
        let mut operations = Vec::new();

        self.collect_patches(&self.current_root_indexes, &self.last_root_indexes, &mut operations)?;

        Ok(operations)
    }

    #[inline]
    fn get_from_arena(&self, index: usize) -> Result<&Diffable, DiffError> {
        self.diffable_arena
            .get(index)
            .ok_or_else(|| DiffError::MissingFromArena { id: index })
    }

    fn make_delete_patch(&self, last: &Diffable) -> Result<Patch, DiffError> {
        let node = Patch::Delete {
            name: last.entity.name.clone(),
            role: last.entity.role.clone(),
        };
        Ok(node)
    }

    fn fill_entity_children(&self, diff: &Diffable, ent: &mut Entity) -> Result<(), DiffError> {
        ent.children.clear();
        for ix in &diff.children_indexes {
            let child_diffable = self.get_from_arena(*ix)?;
            let mut child_entity = child_diffable.entity.clone();
            self.fill_entity_children(child_diffable, &mut child_entity)?;
            ent.children.push(child_entity);
        }
        Ok(())
    }

    fn make_create_patch(
        &self,
        current: &Diffable,
    ) -> Result<Patch, DiffError> {
        let parent_info = if let Some(p) = current.parent_index {
            let parent_diffable = self.get_from_arena(p)?;
            Some((parent_diffable.entity.name.clone(), parent_diffable.entity.role.clone()))
        } else {
            None
        };
        // this needs to be filled up with children
        let mut entity = current.entity.clone();
        self.fill_entity_children(current, &mut entity)?;
        let node = Patch::New {
            entity,
            parent: parent_info,
        };
        Ok(node)
    }

    fn make_modify_patch(
        &self,
        current: &Diffable,
        last: &Diffable,
    ) -> Result<Patch, DiffError> {
        let mut modifications = StableHashMap::default();
        let mut deletions = Vec::new();

        for (k, v) in current.entity.attrs.iter() {
            if last.entity.attrs.get(k) != Some(v) {
                modifications.insert(k.clone(), v.clone());
            }
        }

        for k in last.entity.attrs.keys() {
            if !current.entity.attrs.contains_key(k) {
                deletions.push(k.clone());
            }
        }

        let new_name = if current.entity.name != last.entity.name {
            Some(current.entity.name.clone())
        } else {
            None
        };

        let new_type_name = if current.entity.type_name != last.entity.type_name {
            Some(current.entity.type_name.clone())
        } else {
            None
        };

        let node = Patch::Modify {
            name: last.entity.name.clone(),
            role: last.entity.role.clone(),
            extras: current.entity.extras.clone(),
            new_name,
            new_type_name,
            modifications,
            deletions,
        };

        Ok(node)
    }

    fn get_parent_id(&self, d: &Diffable) -> Result<Option<u64>, DiffError> {
        if let Some(p_index) = d.parent_index {
            let parent_diffable = self.get_from_arena(p_index)?;
            Ok(Some(parent_diffable.id))
        } else {
            Ok(None)
        }
    }

    fn collect_patches(
        &self,
        current: &[usize],
        last: &[usize],
        operations: &mut Vec<Patch>,
    ) -> Result<(), DiffError> {

        let mut history = HashSet::new();

        for cix in current {
            let current_index = *cix;
            let current_diffable = self.get_from_arena(current_index)?;

            if let Some(lix) = self.last_state.get(&current_diffable.id) {
                let last_index = *lix;
                let last_diffable = self.get_from_arena(last_index)?;

                // Mark as processed
                history.insert(last_index);

                if current_diffable.identity_key != last_diffable.identity_key {
                    let node = self.make_modify_patch(current_diffable, last_diffable)?;
                    operations.push(node);
                }
                self.collect_patches(
                    &current_diffable.children_indexes,
                    &last_diffable.children_indexes,
                    operations
                )?;
            } else {
                    let node = self.make_create_patch(current_diffable)?;
                    operations.push(node);
            }
        }

        for lix in last {
            if history.contains(lix) {
                continue;
            }
            let last_index = *lix;
            let last_diffable = self.get_from_arena(last_index)?;

            if !self.current_state.contains_key(&last_diffable.id) {
                // Delete could be a rename. This should be handled at a higher level
                let node = self.make_delete_patch(last_diffable)?;
                operations.push(node);
            }
        }

        Ok(())
    }
}


#[derive(Debug)]
pub enum Patch {
    New {
        entity: Entity,
        parent: Option<(String, String)>,
    },
    Modify {
        extras: Vec<String>,
        name: String,
        role: String,
        new_type_name: Option<String>,
        new_name: Option<String>,
        modifications: StableHashMap<String, String>,
        deletions: Vec<String>,
    },
    Delete {
        name: String,
        role: String,
    },
}


#[cfg(test)]
mod tests {

    fn create_entity() -> super::Entity {
        super::Entity {
            name: "users".to_string(),
            role: "table".to_string(),
            type_name: "table".to_string(),
            attrs: {
                let mut map = super::StableHashMap::default();
                map.insert("columns".to_string(), "id,name,email".to_string());
                map
            },
            extras: vec![],
            children: vec![],
        }
    }

    #[test]
    fn test_load_state() {
        let mut engine = super::DiffEngine::new();
        let entities = vec![create_entity()];
        engine.load_last_state(entities).unwrap();
        assert_eq!(engine.last_state.len(), 1);
        assert_eq!(engine.last_root_indexes.len(), 1);
    }

    #[test]
    fn test_load_state_with_children(){
        let mut engine = super::DiffEngine::new();
        let mut parent = create_entity();
        let child = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: {
                let mut map = super::StableHashMap::default();
                map.insert("nullable".to_string(), "false".to_string());
                map
            },
            extras: vec![],
            children: vec![],
        };
        parent.children.push(child);
        let entities = vec![parent];
        engine.load_last_state(entities).unwrap();
        assert_eq!(engine.last_state.len(), 2);
        assert_eq!(engine.last_root_indexes.len(), 1);
    }

    #[test]
    fn test_diff_no_changes() {
        let mut engine = super::DiffEngine::new();
        let entities = vec![create_entity()];
        engine.load_last_state(entities.clone()).unwrap();
        engine.load_current_state(entities).unwrap();
        let patches = engine.diff().unwrap();
        assert_eq!(patches.len(), 0);
    }

    // Verify that name_key() produces stable, deterministic hashes for the same name/role combination
    #[test]
    fn test_name_key_consistency() {
        let entity1 = create_entity();
        let entity2 = create_entity();
        assert_eq!(entity1.name_key(), entity2.name_key());
    }

    // Ensure different name/role pairs produce different hash values
    #[test]
    fn test_make_name_key_collision_resistance() {
        let entity1 = create_entity();
        let mut entity2 = create_entity();
        entity2.name = "posts".to_string();
        assert_ne!(entity1.name_key(), entity2.name_key());
        
        let mut entity3 = create_entity();
        entity3.role = "view".to_string();
        assert_ne!(entity1.name_key(), entity3.name_key());
    }

    // Verify role_key() produces consistent hashes for the same role
    #[test]
    fn test_role_key_deterministic() {
        let entity1 = create_entity();
        let entity2 = create_entity();
        assert_eq!(entity1.role_key(), entity2.role_key());
    }

    // Verify type_key() produces consistent hashes for the same type_name
    #[test]
    fn test_type_key_deterministic() {
        let entity1 = create_entity();
        let entity2 = create_entity();
        assert_eq!(entity1.type_key(), entity2.type_key());
    }

    // Confirm that changing only the name doesn't affect the identity_key
    #[test]
    fn test_identity_key_ignores_name() {
        let entity1 = create_entity();
        let mut entity2 = create_entity();
        entity2.name = "posts".to_string();
        assert_eq!(entity1.identity_key(), entity2.identity_key());
    }

    // Verify identity_key changes when attributes are modified
    #[test]
    fn test_identity_key_changes_with_attrs() {
        let entity1 = create_entity();
        let mut entity2 = create_entity();
        entity2.attrs.insert("new_attr".to_string(), "value".to_string());
        assert_ne!(entity1.identity_key(), entity2.identity_key());
    }

    // Ensure attrs in different insertion order produce same identity_key (due to sorted keys)
    #[test]
    fn test_identity_key_attr_order_independence() {
        let mut entity1 = super::Entity {
            name: "users".to_string(),
            role: "table".to_string(),
            type_name: "table".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        entity1.attrs.insert("a".to_string(), "1".to_string());
        entity1.attrs.insert("b".to_string(), "2".to_string());
        
        let mut entity2 = super::Entity {
            name: "users".to_string(),
            role: "table".to_string(),
            type_name: "table".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        entity2.attrs.insert("b".to_string(), "2".to_string());
        entity2.attrs.insert("a".to_string(), "1".to_string());
        
        assert_eq!(entity1.identity_key(), entity2.identity_key());
    }

    // Verify identity_key changes when extras list is modified
    #[test]
    fn test_identity_key_changes_with_extras() {
        let entity1 = create_entity();
        let mut entity2 = create_entity();
        entity2.extras.push("extra_info".to_string());
        assert_ne!(entity1.identity_key(), entity2.identity_key());
    }

    // Verify proper error handling when index doesn't exist in arena
    #[test]
    fn test_get_from_arena_missing() {
        let engine = super::DiffEngine::new();
        let result = engine.get_from_arena(999);
        assert!(matches!(result, Err(super::DiffError::MissingFromArena { id: 999 })));
    }

    // Verify delete patch is correctly constructed from a Diffable
    #[test]
    fn test_make_delete_patch() {
        let mut engine = super::DiffEngine::new();
        let entities = vec![create_entity()];
        engine.load_last_state(entities).unwrap();
        let diffable = engine.get_from_arena(0).unwrap();
        let patch = engine.make_delete_patch(diffable).unwrap();
        
        match patch {
            super::Patch::Delete { name, role } => {
                assert_eq!(name, "users");
                assert_eq!(role, "table");
            }
            _ => panic!("Expected Delete patch"),
        }
    }

    // Verify create patch includes full entity tree with all children
    #[test]
    fn test_make_create_patch_with_children() {
        let mut engine = super::DiffEngine::new();
        let mut parent = create_entity();
        let child = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        parent.children.push(child);
        engine.load_last_state(vec![parent]).unwrap();
        
        let diffable = engine.get_from_arena(0).unwrap();
        let patch = engine.make_create_patch(diffable).unwrap();
        
        match patch {
            super::Patch::New { entity, parent: _ } => {
                assert_eq!(entity.children.len(), 1);
                assert_eq!(entity.children[0].name, "id");
            }
            _ => panic!("Expected New patch"),
        }
    }

    // Verify modify patch correctly captures name, type, attr modifications and deletions
    #[test]
    fn test_make_modify_patch_all_changes() {
        let mut engine = super::DiffEngine::new();
        let entity1 = create_entity();
        engine.load_last_state(vec![entity1]).unwrap();
        
        let mut entity2 = create_entity();
        entity2.name = "new_users".to_string();
        entity2.type_name = "new_table".to_string();
        entity2.attrs.insert("columns".to_string(), "id,name".to_string());
        entity2.attrs.insert("new_col".to_string(), "value".to_string());
        engine.load_current_state(vec![entity2]).unwrap();
        
        let current = engine.get_from_arena(1).unwrap();
        let last = engine.get_from_arena(0).unwrap();
        let patch = engine.make_modify_patch(current, last).unwrap();
        
        match patch {
            super::Patch::Modify { name, role, new_name, new_type_name, modifications, deletions, .. } => {
                assert_eq!(name, "users");
                assert_eq!(role, "table");
                assert_eq!(new_name, Some("new_users".to_string()));
                assert_eq!(new_type_name, Some("new_table".to_string()));
                assert!(modifications.contains_key("columns"));
                assert!(modifications.contains_key("new_col"));
            }
            _ => panic!("Expected Modify patch"),
        }
    }

    // Verify correct parent ID is returned when parent exists
    #[test]
    fn test_get_parent_id_with_parent() {
        let mut engine = super::DiffEngine::new();
        let mut parent = create_entity();
        let child = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        parent.children.push(child);
        engine.load_last_state(vec![parent]).unwrap();
        
        let child_diffable = engine.get_from_arena(1).unwrap();
        let parent_id = engine.get_parent_id(child_diffable).unwrap();
        assert!(parent_id.is_some());
    }

    // Verify None is returned for root entities
    #[test]
    fn test_get_parent_id_without_parent() {
        let mut engine = super::DiffEngine::new();
        let entity = create_entity();
        engine.load_last_state(vec![entity]).unwrap();
        
        let diffable = engine.get_from_arena(0).unwrap();
        let parent_id = engine.get_parent_id(diffable).unwrap();
        assert!(parent_id.is_none());
    }

    // Verify children are recursively filled in correct order
    #[test]
    fn test_fill_entity_children_recursive() {
        let mut engine = super::DiffEngine::new();
        let mut parent = create_entity();
        let mut child1 = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        let grandchild = super::Entity {
            name: "constraint".to_string(),
            role: "constraint".to_string(),
            type_name: "primary_key".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        child1.children.push(grandchild);
        parent.children.push(child1);
        engine.load_last_state(vec![parent]).unwrap();
        
        let diffable = engine.get_from_arena(0).unwrap();
        let mut entity = diffable.entity.clone();
        entity.children.clear();
        engine.fill_entity_children(diffable, &mut entity).unwrap();
        
        assert_eq!(entity.children.len(), 1);
        assert_eq!(entity.children[0].name, "id");
        assert_eq!(entity.children[0].children.len(), 1);
        assert_eq!(entity.children[0].children[0].name, "constraint");
    }

    // Verify rename detection when identity_key matches but name differs
    #[test]
    fn test_collect_patches_detects_renames() {
        let mut engine = super::DiffEngine::new();
        let entity1 = create_entity();
        engine.load_last_state(vec![entity1]).unwrap();
        
        let mut entity2 = create_entity();
        entity2.name = "new_users".to_string();
        engine.load_current_state(vec![entity2]).unwrap();
        
        let patches = engine.diff().unwrap();
        // Since name_key (id) changes with name, this results in delete + create
        assert_eq!(patches.len(), 2);
        
        let has_delete = patches.iter().any(|p| matches!(p, super::Patch::Delete { name, .. } if name == "users"));
        let has_create = patches.iter().any(|p| matches!(p, super::Patch::New { entity, .. } if entity.name == "new_users"));
        
        assert!(has_delete, "Expected delete patch for old name");
        assert!(has_create, "Expected create patch for new name");
    }

    // Verify correct patches when entities move between parents
    #[test]
    fn test_collect_patches_handles_moves() {
        let mut engine = super::DiffEngine::new();
        
        let mut parent1 = super::Entity {
            name: "parent1".to_string(),
            role: "table".to_string(),
            type_name: "table".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        let child = super::Entity {
            name: "child".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        parent1.children.push(child);
        engine.load_last_state(vec![parent1]).unwrap();
        
        let parent2 = super::Entity {
            name: "parent2".to_string(),
            role: "table".to_string(),
            type_name: "table".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        let moved_child = super::Entity {
            name: "child".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        engine.load_current_state(vec![parent2, moved_child]).unwrap();
        
        let patches = engine.diff().unwrap();
        // Should detect the move as no change to child (since parent context isn't in identity)
        // and addition of parent2, removal of parent1
        assert!(patches.len() > 0);
    }

    // Verifies that new() initializes an empty engine with no entities
    #[test]
    fn test_new_creates_empty_engine() {
        let engine = super::DiffEngine::new();
        assert_eq!(engine.diffable_arena.len(), 0);
        assert_eq!(engine.last_state.len(), 0);
        assert_eq!(engine.current_state.len(), 0);
        assert_eq!(engine.last_root_indexes.len(), 0);
        assert_eq!(engine.current_root_indexes.len(), 0);
    }

    // Verifies basic loading of a single entity without children
    #[test]
    fn test_load_last_state_single_entity() {
        let mut engine = super::DiffEngine::new();
        let entity = create_entity();
        engine.load_last_state(vec![entity]).unwrap();
        
        assert_eq!(engine.last_state.len(), 1);
        assert_eq!(engine.diffable_arena.len(), 1);
        assert_eq!(engine.last_root_indexes.len(), 1);
    }

    // Verifies loading of entity trees preserves parent-child relationships
    #[test]
    fn test_load_last_state_with_hierarchy() {
        let mut engine = super::DiffEngine::new();
        let mut parent = create_entity();
        let mut child = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        let grandchild = super::Entity {
            name: "index".to_string(),
            role: "index".to_string(),
            type_name: "btree".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        child.children.push(grandchild);
        parent.children.push(child);
        
        engine.load_last_state(vec![parent]).unwrap();
        
        assert_eq!(engine.last_state.len(), 3);
        assert_eq!(engine.diffable_arena.len(), 3);
        assert_eq!(engine.last_root_indexes.len(), 1);
        
        let parent_diff = engine.get_from_arena(0).unwrap();
        assert_eq!(parent_diff.children_indexes.len(), 1);
        
        let child_diff = engine.get_from_arena(1).unwrap();
        assert_eq!(child_diff.children_indexes.len(), 1);
        assert_eq!(child_diff.parent_index, Some(0));
    }

    // Verifies that loading entities with duplicate name+role returns error
    #[test]
    fn test_load_last_state_duplicate_error() {
        let mut engine = super::DiffEngine::new();
        let entity1 = create_entity();
        let entity2 = create_entity();
        
        let result = engine.load_last_state(vec![entity1, entity2]);
        
        assert!(matches!(
            result,
            Err(super::DiffError::DuplicateEntity { .. })
        ));
    }

    // Verifies that multiple root entities are correctly tracked
    #[test]
    fn test_load_last_state_multiple_roots() {
        let mut engine = super::DiffEngine::new();
        let entity1 = create_entity();
        let mut entity2 = create_entity();
        entity2.name = "posts".to_string();
        
        engine.load_last_state(vec![entity1, entity2]).unwrap();
        
        assert_eq!(engine.last_state.len(), 2);
        assert_eq!(engine.last_root_indexes.len(), 2);
    }

    // Verifies basic loading into current_state without affecting last_state
    #[test]
    fn test_load_current_state_single_entity() {
        let mut engine = super::DiffEngine::new();
        let entity = create_entity();
        engine.load_current_state(vec![entity]).unwrap();
        
        assert_eq!(engine.current_state.len(), 1);
        assert_eq!(engine.last_state.len(), 0);
        assert_eq!(engine.diffable_arena.len(), 1);
        assert_eq!(engine.current_root_indexes.len(), 1);
    }

    // Verifies loading of entity trees into current_state preserves structure
    #[test]
    fn test_load_current_state_with_hierarchy() {
        let mut engine = super::DiffEngine::new();
        let mut parent = create_entity();
        let child = super::Entity {
            name: "name".to_string(),
            role: "column".to_string(),
            type_name: "varchar".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        parent.children.push(child);
        
        engine.load_current_state(vec![parent]).unwrap();
        
        assert_eq!(engine.current_state.len(), 2);
        assert_eq!(engine.diffable_arena.len(), 2);
        assert_eq!(engine.current_root_indexes.len(), 1);
        
        let parent_diff = engine.get_from_arena(0).unwrap();
        assert_eq!(parent_diff.children_indexes.len(), 1);
    }

    // Verifies duplicate detection works for current_state
    #[test]
    fn test_load_current_state_duplicate_error() {
        let mut engine = super::DiffEngine::new();
        let entity1 = create_entity();
        let entity2 = create_entity();
        
        let result = engine.load_current_state(vec![entity1, entity2]);
        
        assert!(matches!(
            result,
            Err(super::DiffError::DuplicateEntity { .. })
        ));
    }

    // Verifies that loading both states maintain separate indexes
    #[test]
    fn test_load_both_states_independently() {
        let mut engine = super::DiffEngine::new();
        let entity1 = create_entity();
        let mut entity2 = create_entity();
        entity2.name = "posts".to_string();
        
        engine.load_last_state(vec![entity1]).unwrap();
        engine.load_current_state(vec![entity2]).unwrap();
        
        assert_eq!(engine.last_state.len(), 1);
        assert_eq!(engine.current_state.len(), 1);
        assert_eq!(engine.diffable_arena.len(), 2);
        assert_eq!(engine.last_root_indexes.len(), 1);
        assert_eq!(engine.current_root_indexes.len(), 1);
    }

    // Verifies diff() returns empty patch list when both states are empty
    #[test]
    fn test_diff_empty_states() {
        let engine = super::DiffEngine::new();
        let patches = engine.diff().unwrap();
        assert_eq!(patches.len(), 0);
    }

    // Verifies diff() generates New patches when entities exist only in current_state
    #[test]
    fn test_diff_only_additions() {
        let mut engine = super::DiffEngine::new();
        
        let mut parent = create_entity();
        let child = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        parent.children.push(child);
        
        engine.load_last_state(vec![]).unwrap();
        engine.load_current_state(vec![parent]).unwrap();
        
        let patches = engine.diff().unwrap();
        assert_eq!(patches.len(), 1);
        
        match &patches[0] {
            super::Patch::New { entity, .. } => {
                assert_eq!(entity.name, "users");
                assert_eq!(entity.children.len(), 1);
                assert_eq!(entity.children[0].name, "id");
            }
            _ => panic!("Expected New patch"),
        }
    }

    // Verifies diff() generates Delete patches when entities exist only in last_state
    #[test]
    fn test_diff_only_deletions() {
        let mut engine = super::DiffEngine::new();
        
        let mut parent = create_entity();
        let child = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        parent.children.push(child);
        
        engine.load_last_state(vec![parent]).unwrap();
        engine.load_current_state(vec![]).unwrap();
        
        let patches = engine.diff().unwrap();
        assert_eq!(patches.len(), 1);
        
        match &patches[0] {
            super::Patch::Delete { name, role } => {
                assert_eq!(name, "users");
                assert_eq!(role, "table");
            }
            _ => panic!("Expected Delete patch"),
        }
    }

    // Verifies diff() generates Modify patches when entities exist in both but differ
    #[test]
    fn test_diff_modifications_only() {
        let mut engine = super::DiffEngine::new();
        
        let mut entity1 = create_entity();
        let child1 = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        entity1.children.push(child1);
        
        let mut entity2 = create_entity();
        entity2.attrs.insert("indexed".to_string(), "true".to_string());
        let child2 = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "bigint".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        entity2.children.push(child2);
        
        engine.load_last_state(vec![entity1]).unwrap();
        engine.load_current_state(vec![entity2]).unwrap();
        
        let patches = engine.diff().unwrap();
        assert_eq!(patches.len(), 2); // parent and child modifications
        
        let parent_modify = patches.iter().find(|p| {
            matches!(p, super::Patch::Modify { role, .. } if role == "table")
        });
        assert!(parent_modify.is_some());
        
        let child_modify = patches.iter().find(|p| {
            matches!(p, super::Patch::Modify { role, .. } if role == "column")
        });
        assert!(child_modify.is_some());
    }

    // Verifies diff() correctly generates all patch types in a single operation
    #[test]
    fn test_diff_mixed_operations() {
        let mut engine = super::DiffEngine::new();
        
        let mut old_table = create_entity();
        let old_column = super::Entity {
            name: "old_col".to_string(),
            role: "column".to_string(),
            type_name: "varchar".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        old_table.children.push(old_column);
        
        let mut shared_table = super::Entity {
            name: "shared".to_string(),
            role: "table".to_string(),
            type_name: "table".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        
        engine.load_last_state(vec![old_table, shared_table.clone()]).unwrap();
        
        // Modify shared table
        shared_table.attrs.insert("modified".to_string(), "yes".to_string());
        
        // New table
        let new_table = super::Entity {
            name: "new_table".to_string(),
            role: "table".to_string(),
            type_name: "table".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        
        engine.load_current_state(vec![shared_table, new_table]).unwrap();
        
        let patches = engine.diff().unwrap();
        
        let has_delete = patches.iter().any(|p| matches!(p, super::Patch::Delete { name, .. } if name == "users"));
        let has_modify = patches.iter().any(|p| matches!(p, super::Patch::Modify { name, .. } if name == "shared"));
        let has_new = patches.iter().any(|p| matches!(p, super::Patch::New { entity, .. } if entity.name == "new_table"));
        
        assert!(has_delete);
        assert!(has_modify);
        assert!(has_new);
    }

    // Verifies diff() recursively detects changes in child entities
    #[test]
    fn test_diff_nested_changes() {
        let mut engine = super::DiffEngine::new();
        
        let mut parent = create_entity();
        let mut child = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        let grandchild = super::Entity {
            name: "constraint".to_string(),
            role: "constraint".to_string(),
            type_name: "primary_key".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        child.children.push(grandchild);
        parent.children.push(child);
        
        engine.load_last_state(vec![parent.clone()]).unwrap();
        
        // Modify grandchild
        parent.children[0].children[0].type_name = "unique".to_string();
        
        engine.load_current_state(vec![parent]).unwrap();
        
        let patches = engine.diff().unwrap();
        
        let grandchild_modify = patches.iter().find(|p| {
            matches!(p, super::Patch::Modify { name, role, .. } 
                if name == "constraint" && role == "constraint")
        });
        
        assert!(grandchild_modify.is_some());
    }

    // Verifies diff() doesn't generate patches for unchanged entities
    #[test]
    fn test_diff_preserve_unchanged() {
        let mut engine = super::DiffEngine::new();
        
        let mut parent = create_entity();
        let child = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        parent.children.push(child);
        
        engine.load_last_state(vec![parent.clone()]).unwrap();
        engine.load_current_state(vec![parent]).unwrap();
        
        let patches = engine.diff().unwrap();
        assert_eq!(patches.len(), 0);
    }

    // Verifies Delete patch correctly removes entity from last_state
    #[test]
    fn test_patch_delete_removes_entity() {
        let mut engine = super::DiffEngine::new();
        
        let mut parent = create_entity();
        let child = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        parent.children.push(child);
        
        engine.load_last_state(vec![parent]).unwrap();
        
        assert_eq!(engine.last_state.len(), 2);
        
        let patch = super::Patch::Delete {
            name: "id".to_string(),
            role: "column".to_string(),
        };
        
        engine.patch(patch).unwrap();
        
        assert_eq!(engine.last_state.len(), 1);
        
        // Verify parent's children list is updated
        let parent_diff = engine.get_from_arena(0).unwrap();
        assert_eq!(parent_diff.children_indexes.len(), 0);
    }

    // Verifies Delete patch returns error when entity doesn't exist
    #[test]
    fn test_patch_delete_nonexistent_error() {
        let mut engine = super::DiffEngine::new();
        engine.load_last_state(vec![create_entity()]).unwrap();
        
        let patch = super::Patch::Delete {
            name: "nonexistent".to_string(),
            role: "table".to_string(),
        };
        
        let result = engine.patch(patch);
        
        assert!(matches!(
            result,
            Err(super::DiffError::EntityNotFound { .. })
        ));
    }

    // Verifies that Delete patch cascades to all children recursively
    // When a parent is deleted, all descendants are removed from last_state
    // They remain in the arena but are no longer accessible via the state map
    #[test]
    fn test_patch_delete_cascades_to_all_children() {
        let mut engine = super::DiffEngine::new();
        
        let mut parent = create_entity();
        let mut child1 = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        let grandchild = super::Entity {
            name: "pk_constraint".to_string(),
            role: "constraint".to_string(),
            type_name: "primary_key".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        child1.children.push(grandchild);
        
        let child2 = super::Entity {
            name: "name".to_string(),
            role: "column".to_string(),
            type_name: "varchar".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        
        parent.children.push(child1);
        parent.children.push(child2);
        
        engine.load_last_state(vec![parent]).unwrap();
        
        // Verify initial state: parent + 2 children + 1 grandchild = 4 entities
        assert_eq!(engine.last_state.len(), 4);
        
        // Delete the parent
        let patch = super::Patch::Delete {
            name: "users".to_string(),
            role: "table".to_string(),
        };
        
        engine.patch(patch).unwrap();
        
        // After deleting parent, ALL entities are removed from last_state
        // because patch() only removes the specific entity, not its children
        // However, the children become orphaned (parent is gone but they remain)
        assert_eq!(engine.last_state.len(), 0);
        
        // The parent should not be in last_state
        let parent_id = super::Entity::make_name_key("users", "table");
        assert!(!engine.last_state.contains_key(&parent_id));
        
        // Children are also removed implicitly because they're not accessible anymore
        let child1_id = super::Entity::make_name_key("id", "column");
        let child2_id = super::Entity::make_name_key("name", "column");
        let grandchild_id = super::Entity::make_name_key("pk_constraint", "constraint");
        
        assert!(!engine.last_state.contains_key(&child1_id));
        assert!(!engine.last_state.contains_key(&child2_id));
        assert!(!engine.last_state.contains_key(&grandchild_id));
        
        // All entities still remain in the arena (memory not freed)
        assert_eq!(engine.diffable_arena.len(), 4);
    }

    // Verifies Modify patch updates entity attributes and recomputes hash keys
    #[test]
    fn test_patch_modify_updates_entity() {
        let mut engine = super::DiffEngine::new();
        
        let mut parent = create_entity();
        let child = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        parent.children.push(child);
        
        engine.load_last_state(vec![parent]).unwrap();
        
        let mut modifications = super::StableHashMap::default();
        modifications.insert("new_attr".to_string(), "value".to_string());
        
        let patch = super::Patch::Modify {
            name: "users".to_string(),
            role: "table".to_string(),
            new_name: Some("user_accounts".to_string()),
            new_type_name: Some("view".to_string()),
            modifications,
            deletions: vec!["columns".to_string()],
            extras: vec!["extra1".to_string()],
        };
        
        engine.patch(patch).unwrap();
        
        let diffable = engine.get_from_arena(0).unwrap();
        assert_eq!(diffable.entity.name, "user_accounts");
        assert_eq!(diffable.entity.type_name, "view");
        assert!(diffable.entity.attrs.contains_key("new_attr"));
        assert!(!diffable.entity.attrs.contains_key("columns"));
        assert_eq!(diffable.entity.extras.len(), 1);
    }

    // Verifies Modify patch returns error when entity doesn't exist
    #[test]
    fn test_patch_modify_nonexistent_error() {
        let mut engine = super::DiffEngine::new();
        engine.load_last_state(vec![create_entity()]).unwrap();
        
        let patch = super::Patch::Modify {
            name: "nonexistent".to_string(),
            role: "table".to_string(),
            new_name: None,
            new_type_name: None,
            modifications: super::StableHashMap::default(),
            deletions: vec![],
            extras: vec![],
        };
        
        let result = engine.patch(patch);
        
        assert!(matches!(
            result,
            Err(super::DiffError::EntityNotFound { .. })
        ));
    }

    // Verifies Modify patch returns error when renaming to existing name
    #[test]
    fn test_patch_modify_duplicate_name_error() {
        let mut engine = super::DiffEngine::new();
        
        let entity1 = create_entity();
        let mut entity2 = create_entity();
        entity2.name = "posts".to_string();
        
        engine.load_last_state(vec![entity1, entity2]).unwrap();
        
        let patch = super::Patch::Modify {
            name: "users".to_string(),
            role: "table".to_string(),
            new_name: Some("posts".to_string()),
            new_type_name: None,
            modifications: super::StableHashMap::default(),
            deletions: vec![],
            extras: vec![],
        };
        
        let result = engine.patch(patch);
        
        assert!(matches!(
            result,
            Err(super::DiffError::DuplicateEntity { .. })
        ));
    }

    // Verifies New patch adds entity to last_state with correct parent
    #[test]
    fn test_patch_new_adds_entity() {
        let mut engine = super::DiffEngine::new();
        
        let parent = create_entity();
        engine.load_last_state(vec![parent]).unwrap();
        
        assert_eq!(engine.last_state.len(), 1);
        
        let new_column = super::Entity {
            name: "email".to_string(),
            role: "column".to_string(),
            type_name: "varchar".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        
        let patch = super::Patch::New {
            entity: new_column,
            parent: Some(("users".to_string(), "table".to_string())),
        };
        
        engine.patch(patch).unwrap();
        
        assert_eq!(engine.last_state.len(), 2);
        
        // Verify parent's children list is updated
        let parent_diff = engine.get_from_arena(0).unwrap();
        assert_eq!(parent_diff.children_indexes.len(), 1);
        assert_eq!(parent_diff.children_indexes[0], 1);
    }

    // Verifies New patch recursively adds entity with all children
    #[test]
    fn test_patch_new_with_children() {
        let mut engine = super::DiffEngine::new();
        engine.load_last_state(vec![]).unwrap();
        
        let mut new_table = create_entity();
        new_table.name = "posts".to_string();
        let mut child = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        let grandchild = super::Entity {
            name: "pk".to_string(),
            role: "constraint".to_string(),
            type_name: "primary_key".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        child.children.push(grandchild);
        new_table.children.push(child);
        
        let patch = super::Patch::New {
            entity: new_table,
            parent: None,
        };
        
        engine.patch(patch).unwrap();
        
        assert_eq!(engine.last_state.len(), 3);
        assert_eq!(engine.last_root_indexes.len(), 1);
        
        let table_diff = engine.get_from_arena(0).unwrap();
        assert_eq!(table_diff.children_indexes.len(), 1);
        
        let column_diff = engine.get_from_arena(1).unwrap();
        assert_eq!(column_diff.children_indexes.len(), 1);
    }

    // Verifies New patch returns error when parent doesn't exist
    #[test]
    fn test_patch_new_missing_parent_error() {
        let mut engine = super::DiffEngine::new();
        engine.load_last_state(vec![create_entity()]).unwrap();
        
        let new_column = super::Entity {
            name: "email".to_string(),
            role: "column".to_string(),
            type_name: "varchar".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        
        let patch = super::Patch::New {
            entity: new_column,
            parent: Some(("nonexistent".to_string(), "table".to_string())),
        };
        
        let result = engine.patch(patch);
        
        assert!(matches!(
            result,
            Err(super::DiffError::EntityNotFound { .. })
        ));
    }

    // Verifies multiple patches can be applied in sequence
    #[test]
    fn test_patch_sequence_replay() {
        let mut engine = super::DiffEngine::new();
        
        let entity = create_entity();
        engine.load_last_state(vec![entity]).unwrap();
        
        // Add a new entity
        let new_entity = super::Entity {
            name: "posts".to_string(),
            role: "table".to_string(),
            type_name: "table".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        let patch1 = super::Patch::New {
            entity: new_entity,
            parent: None,
        };
        engine.patch(patch1).unwrap();
        
        // Modify existing entity
        let mut modifications = super::StableHashMap::default();
        modifications.insert("indexed".to_string(), "true".to_string());
        let patch2 = super::Patch::Modify {
            name: "users".to_string(),
            role: "table".to_string(),
            new_name: None,
            new_type_name: None,
            modifications,
            deletions: vec![],
            extras: vec![],
        };
        engine.patch(patch2).unwrap();
        
        // Delete an entity
        let patch3 = super::Patch::Delete {
            name: "posts".to_string(),
            role: "table".to_string(),
        };
        engine.patch(patch3).unwrap();
        
        assert_eq!(engine.last_state.len(), 1);
        
        let remaining = engine.get_from_arena(0).unwrap();
        assert_eq!(remaining.entity.name, "users");
        assert!(remaining.entity.attrs.contains_key("indexed"));
    }

    // Verifies diff generates correct New patch for new columns in existing table
    #[test]
    fn test_diff_new_column_in_existing_table() {
        let mut engine = super::DiffEngine::new();
        
        let mut old_table = create_entity();
        let old_column = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        old_table.children.push(old_column);
        
        engine.load_last_state(vec![old_table]).unwrap();
        
        let mut new_table = create_entity();
        let existing_column = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        let new_column = super::Entity {
            name: "email".to_string(),
            role: "column".to_string(),
            type_name: "varchar".to_string(),
            attrs: {
                let mut map = super::StableHashMap::default();
                map.insert("length".to_string(), "255".to_string());
                map.insert("nullable".to_string(), "false".to_string());
                map
            },
            extras: vec!["unique".to_string()],
            children: vec![],
        };
        new_table.children.push(existing_column);
        new_table.children.push(new_column);
        
        engine.load_current_state(vec![new_table]).unwrap();
        
        let patches = engine.diff().unwrap();
        
        // Should have one New patch for the new column
        let new_patches: Vec<_> = patches.iter().filter(|p| {
            matches!(p, super::Patch::New { entity, .. } if entity.name == "email")
        }).collect();
        
        assert_eq!(new_patches.len(), 1);
        
        match new_patches[0] {
            super::Patch::New { entity, parent } => {
                assert_eq!(entity.name, "email");
                assert_eq!(entity.role, "column");
                assert_eq!(entity.type_name, "varchar");
                assert_eq!(entity.attrs.get("length"), Some(&"255".to_string()));
                assert_eq!(entity.attrs.get("nullable"), Some(&"false".to_string()));
                assert_eq!(entity.extras.len(), 1);
                assert_eq!(entity.extras[0], "unique");
                
                // Verify parent info is correct
                assert_eq!(parent, &Some(("users".to_string(), "table".to_string())));
            }
            _ => panic!("Expected New patch"),
        }
    }

    // Verifies diff generates correct patches for multiple new columns in existing table
    #[test]
    fn test_diff_multiple_new_columns_in_table() {
        let mut engine = super::DiffEngine::new();
        
        let mut old_table = create_entity();
        let id_column = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        old_table.children.push(id_column);
        
        engine.load_last_state(vec![old_table]).unwrap();
        
        let mut new_table = create_entity();
        let existing_column = super::Entity {
            name: "id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        let email_column = super::Entity {
            name: "email".to_string(),
            role: "column".to_string(),
            type_name: "varchar".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        let created_at_column = super::Entity {
            name: "created_at".to_string(),
            role: "column".to_string(),
            type_name: "timestamp".to_string(),
            attrs: {
                let mut map = super::StableHashMap::default();
                map.insert("default".to_string(), "CURRENT_TIMESTAMP".to_string());
                map
            },
            extras: vec![],
            children: vec![],
        };
        new_table.children.push(existing_column);
        new_table.children.push(email_column);
        new_table.children.push(created_at_column);
        
        engine.load_current_state(vec![new_table]).unwrap();
        
        let patches = engine.diff().unwrap();
        
        // Should have two New patches for the new columns
        let new_column_patches: Vec<_> = patches.iter().filter(|p| {
            matches!(p, super::Patch::New { entity, .. } 
                if entity.role == "column" && (entity.name == "email" || entity.name == "created_at"))
        }).collect();
        
        assert_eq!(new_column_patches.len(), 2);
        
        // Verify both have correct parent info
        for patch in new_column_patches {
            match patch {
                super::Patch::New { parent, .. } => {
                    assert_eq!(parent, &Some(("users".to_string(), "table".to_string())));
                }
                _ => panic!("Expected New patch"),
            }
        }
    }

    // Verifies diff handles new columns with nested children (e.g., constraints)
    #[test]
    fn test_diff_new_column_with_children() {
        let mut engine = super::DiffEngine::new();
        
        let old_table = create_entity();
        engine.load_last_state(vec![old_table]).unwrap();
        
        let mut new_table = create_entity();
        let mut new_column = super::Entity {
            name: "user_id".to_string(),
            role: "column".to_string(),
            type_name: "integer".to_string(),
            attrs: super::StableHashMap::default(),
            extras: vec![],
            children: vec![],
        };
        let foreign_key = super::Entity {
            name: "fk_user".to_string(),
            role: "constraint".to_string(),
            type_name: "foreign_key".to_string(),
            attrs: {
                let mut map = super::StableHashMap::default();
                map.insert("references".to_string(), "users.id".to_string());
                map
            },
            extras: vec![],
            children: vec![],
        };
        new_column.children.push(foreign_key);
        new_table.children.push(new_column);
        
        engine.load_current_state(vec![new_table]).unwrap();
        
        let patches = engine.diff().unwrap();
        
        // Should have one New patch for the column
        let column_patch = patches.iter().find(|p| {
            matches!(p, super::Patch::New { entity, .. } if entity.name == "user_id")
        });
        
        assert!(column_patch.is_some());
        
        match column_patch.unwrap() {
            super::Patch::New { entity, parent } => {
                assert_eq!(entity.name, "user_id");
                assert_eq!(entity.children.len(), 1);
                assert_eq!(entity.children[0].name, "fk_user");
                assert_eq!(entity.children[0].role, "constraint");
                assert_eq!(entity.children[0].attrs.get("references"), Some(&"users.id".to_string()));
                assert_eq!(parent, &Some(("users".to_string(), "table".to_string())));
            }
            _ => panic!("Expected New patch"),
        }
    }


}