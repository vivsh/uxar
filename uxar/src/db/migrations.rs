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

}