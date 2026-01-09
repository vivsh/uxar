use indexmap::IndexMap;
use std::sync::Arc;

use crate::db::{TableModel, models::{OpaqueModel, OpaqueType, QualifiedName}};

use super::actions::Action;
use super::toposort::{toposort_indices, TopoError, TopoNode};

/// Database schema state at a point in time.
/// Build from user models or replay migrations. Diff states to generate actions.
#[derive(Debug, Clone)]
pub struct MigState {
    /// Tables indexed by name
    pub tables: IndexMap<QualifiedName, Arc<TableModel>>,
    /// Opaque objects (views, functions, etc.) indexed by name
    pub opaques: IndexMap<(OpaqueType, QualifiedName), Arc<OpaqueModel>>,
}

impl MigState {
    /// Create empty state
    pub fn new() -> Self {
        Self {
            tables: IndexMap::new(),
            opaques: IndexMap::new(),
        }
    }

    /// Create state with pre-allocated capacity
    pub fn with_capacity(table_count: usize, opaque_count: usize) -> Self {
        Self {
            tables: IndexMap::with_capacity(table_count),
            opaques: IndexMap::with_capacity(opaque_count),
        }
    }

    /// Add or update table
    pub fn add_table(&mut self, table: TableModel) {
        let key = table.qualified_name().clone();
        self.tables.insert(key, Arc::new(table));
    }

    /// Add or update opaque object
    pub fn add_opaque(&mut self, opaque: OpaqueModel) {
        let key = (opaque.kind.clone(), opaque.qualified_name().clone());
        self.opaques.insert(key, Arc::new(opaque));
    }

    /// Remove a table from the state
    pub fn remove_table(&mut self, name: &QualifiedName) -> Option<Arc<TableModel>> {
        self.tables.shift_remove(name)
    }

    /// Remove an opaque object from the state
    pub fn remove_opaque(&mut self, key: &(OpaqueType, QualifiedName)) -> Option<Arc<OpaqueModel>> {
        self.opaques.shift_remove(key)
    }

    /// Get a table by name
    pub fn get_table(&self, name: &QualifiedName) -> Option<&Arc<TableModel>> {
        self.tables.get(name)
    }

    /// Get an opaque object by name
    pub fn get_opaque(&self, key: &(OpaqueType, QualifiedName)) -> Option<&Arc<OpaqueModel>> {
        self.opaques.get(key)
    }

    /// Compare with old state, generate actions for migration from old to self.
    /// Returns actions where: apply(actions, old_state) = self
    /// 
    /// Actions are ordered to prevent dependency failures:
    /// 1. Drop opaques (views/triggers depend on tables)
    /// 2. Drop/alter columns (destructive changes)
    /// 3. Drop tables
    /// 4. Create tables
    /// 5. Add/alter columns (additive changes)
    /// 6. Create/replace opaques (depend on tables)
    pub fn diff_from(&self, old_state: &MigState) -> Result<Vec<Action>, StateError> {
        let mut drop_opaques = Vec::new();
        let mut drop_columns = Vec::new();
        let mut alter_columns = Vec::new();
        let mut drop_tables = Vec::new();
        let mut create_tables = Vec::new();
        let mut create_columns = Vec::new();
        let mut create_opaques = Vec::new();

        // Find dropped opaques (must happen before dropping tables)
        for (key, old_opaque) in &old_state.opaques {
            if !self.opaques.contains_key(key) {
                drop_opaques.push(Action::DropOpaque {
                    name: old_opaque.qualified_name().clone(),
                    kind: old_opaque.kind.clone(),
                });
            }
        }

        // Find dropped tables
        for (name, _old_table) in &old_state.tables {
            if !self.tables.contains_key(name) {
                drop_tables.push(Action::DropTable {
                    name: name.clone(),
                });
            }
        }

        // Find new and modified tables
        for (name, new_table) in &self.tables {
            match old_state.tables.get(name) {
                None => {
                    // New table
                    create_tables.push(Action::CreateTable {
                        model: Arc::clone(new_table),
                    });
                }
                Some(old_table) => {
                    // Always diff columns when table exists in both states (cheap, safe)
                    let column_actions = diff_columns(&name.to_string(), old_table, new_table);
                    // Bucket column actions: drops/destructive first, creates/additive last
                    for action in column_actions {
                        match action {
                            Action::DropColumn { .. } => drop_columns.push(action),
                            Action::AlterColumn { .. } => alter_columns.push(action),
                            Action::CreateColumn { .. } => create_columns.push(action),
                            _ => {} // unreachable from diff_columns
                        }
                    }
                }
            }
        }

        // Find new and modified opaques (must happen after creating tables)
        for (name, new_opaque) in &self.opaques {
            match old_state.opaques.get(name) {
                None => {
                    // New opaque
                    create_opaques.push(Action::CreateOpaque {
                        model: Arc::clone(new_opaque),
                    });
                }
                Some(old_opaque) => {
                    // Check if modified
                    if **old_opaque != **new_opaque {
                        create_opaques.push(Action::ReplaceOpaque {
                            model: Arc::clone(new_opaque),
                        });
                    }
                }
            }
        }

        // Assemble in safe order
        let mut actions = Vec::with_capacity(
            drop_opaques.len() + drop_columns.len() + alter_columns.len() + 
            drop_tables.len() + create_tables.len() + create_columns.len() + 
            create_opaques.len()
        );
        
        // Sort tables by FK dependencies
        let create_tables = toposort_create_tables(create_tables)?;
        let drop_tables = toposort_drop_tables(drop_tables, &old_state.tables)?;
        
        // Sort opaques by dependencies
        let create_opaques = toposort_create_opaques(create_opaques, &self.tables)?;
        let drop_opaques = toposort_drop_opaques(drop_opaques, &old_state.opaques)?;
        
        actions.extend(drop_opaques);
        actions.extend(drop_columns);
        actions.extend(alter_columns);
        actions.extend(drop_tables);
        actions.extend(create_tables);
        actions.extend(create_columns);
        actions.extend(create_opaques);

        Ok(actions)
    }

    /// Apply action to state with validation. Returns error if invalid.
    pub fn apply_action(&mut self, action: &Action) -> Result<(), StateError> {
        match action {
            Action::CreateTable { model } => {
                let key = model.qualified_name().clone();
                if self.tables.contains_key(&key) {
                    return Err(StateError::TableExists(model.qualified_name().to_string()));
                }
                self.tables.insert(key, Arc::clone(model));
            }
            Action::DropTable { name } => {
                if self.tables.shift_remove(name).is_none() {
                    return Err(StateError::TableNotFound(name.to_string()));
                }
            }
            Action::AlterTable { name, delta } => {
                let table_arc = self.tables.get(name)
                    .ok_or_else(|| StateError::TableNotFound(name.to_string()))?;
                let mut table = (**table_arc).clone();
                delta.apply(&mut table);
                let key = table.qualified_name().clone();
                self.tables.insert(key, Arc::new(table));
            }
            Action::CreateColumn { table, model } => {
                let table_arc = self.tables.get(table)
                    .ok_or_else(|| StateError::TableNotFound(table.to_string()))?;
                let mut table_model = (**table_arc).clone();
                
                // Check for duplicate column
                if table_model.columns.iter().any(|col| col.name == model.name) {
                    return Err(StateError::DuplicateColumn {
                        table: table.to_string(),
                        column: model.name.to_string(),
                    });
                }
                
                table_model.add_column((**model).clone());
                let key = table_model.qualified_name().clone();
                self.tables.insert(key, Arc::new(table_model));
            }
            Action::DropColumn { table, name } => {
                let table_arc = self.tables.get(table)
                    .ok_or_else(|| StateError::TableNotFound(table.to_string()))?;
                let mut table_model = (**table_arc).clone();
                
                // Verify column exists before dropping
                let col_count_before = table_model.columns.len();
                table_model.columns.retain(|col| col.name != *name);
                
                if table_model.columns.len() == col_count_before {
                    return Err(StateError::ColumnNotFound(name.to_string()));
                }
                
                let key = table_model.qualified_name().clone();
                self.tables.insert(key, Arc::new(table_model));
            }
            Action::AlterColumn { table, name, delta } => {
                let table_arc = self.tables.get(table)
                    .ok_or_else(|| StateError::TableNotFound(table.to_string()))?;
                let mut table_model = (**table_arc).clone();
                let column = table_model.columns.iter_mut()
                    .find(|col| col.name == *name)
                    .ok_or_else(|| StateError::ColumnNotFound(name.to_string()))?;
                delta.apply(column);
                let key = table_model.qualified_name().clone();
                self.tables.insert(key, Arc::new(table_model));
            }
            Action::CreateOpaque { model } => {
                let key = (model.kind.clone(), model.qualified_name().clone());
                if self.opaques.contains_key(&key) {
                    return Err(StateError::OpaqueExists(model.qualified_name().to_string()));
                }
                self.opaques.insert(key, Arc::clone(model));
            }
            Action::DropOpaque { name, kind } => {
                let key = (kind.clone(), name.clone());
                if self.opaques.shift_remove(&key).is_none() {
                    return Err(StateError::OpaqueNotFound(name.to_string()));
                }
            }
            Action::ReplaceOpaque { model } => {
                let key = (model.kind.clone(), model.qualified_name().clone());
                self.opaques.insert(key, Arc::clone(model));
            }
            Action::RenameTable { old_name, new_name } => {
                let table_arc = self.tables.shift_remove(old_name)
                    .ok_or_else(|| StateError::TableNotFound(old_name.to_string()))?;
                
                let mut table_model = match Arc::try_unwrap(table_arc) {
                    Ok(t) => t,
                    Err(arc) => (*arc).clone(),
                };
                table_model.qualified_name = new_name.clone();
                
                let new_key = table_model.qualified_name.clone();
                self.tables.insert(new_key, Arc::new(table_model));
            }
            Action::RenameColumn { table, old_name, new_name } => {
                let table_arc = self.tables.get(table)
                    .ok_or_else(|| StateError::TableNotFound(table.to_string()))?;
                let mut table_model = match Arc::try_unwrap(Arc::clone(table_arc)) {
                    Ok(t) => t,
                    Err(arc) => (*arc).clone(),
                };
                
                let column = table_model.columns.iter_mut()
                    .find(|col| col.name == *old_name)
                    .ok_or_else(|| StateError::ColumnNotFound(old_name.to_string()))?;
                
                column.name = new_name.clone();
                
                let key = table_model.qualified_name.clone();
                self.tables.insert(key, Arc::new(table_model));
            }
            Action::Statement { .. } => {
                // Raw SQL statements don't affect state tracking
            }
        }
        Ok(())
    }
}

impl Default for MigState {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur when manipulating state
#[derive(Debug, Clone, thiserror::Error)]
pub enum StateError {
    #[error("table '{0}' already exists")]
    TableExists(String),
    #[error("table '{0}' not found")]
    TableNotFound(String),
    #[error("column '{0}' not found")]
    ColumnNotFound(String),
    #[error("column '{column}' already exists in table '{table}'")]
    DuplicateColumn { table: String, column: String },
    #[error("opaque object '{0}' already exists")]
    OpaqueExists(String),
    #[error("opaque object '{0}' not found")]
    OpaqueNotFound(String),
    #[error("circular foreign key dependency detected involving table '{0}'")]
    CircularFKDependency(String),
    #[error("circular opaque dependency detected involving '{0}'")]
    CircularOpaqueDependency(String),
}

/// Compare columns between old and new table, generating actions for differences
fn diff_columns(table_name: &str, old_table: &Arc<TableModel>, new_table: &Arc<TableModel>) -> Vec<Action> {
    let mut actions = Vec::with_capacity(new_table.columns.len().max(old_table.columns.len()));

    // Build maps for efficient lookup
    let old_cols: IndexMap<&str, &_> = old_table.columns.iter()
        .map(|col| (col.name.as_ref(), col))
        .collect();
    let new_cols: IndexMap<&str, &_> = new_table.columns.iter()
        .map(|col| (col.name.as_ref(), col))
        .collect();

    // Find dropped columns
    for (name, _col) in &old_cols {
        if !new_cols.contains_key(name) {
            actions.push(Action::DropColumn {
                table: new_table.qualified_name().clone(),
                name: Arc::from(*name),
            });
        }
    }

    // Find new and modified columns
    for (name, new_col) in &new_cols {
        match old_cols.get(name) {
            None => {
                // New column
                actions.push(Action::CreateColumn {
                    table: new_table.qualified_name().clone(),
                    model: Arc::new((*new_col).clone()),
                });
            }
            Some(old_col) => {
                // Compute delta for modified columns
                if old_col != new_col {
                    if let Some(delta) = compute_column_delta(old_col, new_col) {
                        actions.push(Action::AlterColumn {
                            table: new_table.qualified_name().clone(),
                            name: Arc::from(*name),
                            delta,
                        });
                    }
                }
            }
        }
    }

    actions
}

/// Compute delta between old and new column. Returns None if columns are identical.
/// Delta captures: delta + old = new
/// Note: Name is not included - columns are matched by name, so names are always equal.
/// Column renames are detected as DropColumn + CreateColumn.
fn compute_column_delta(old: &crate::db::ColumnModel, new: &crate::db::ColumnModel) -> Option<crate::db::models::ColumnDelta> {
    use crate::db::models::ColumnDelta;
    
    let mut delta = ColumnDelta::new();
    let mut has_changes = false;

    if old.data_type != new.data_type {
        delta.data_type = Some(new.data_type.clone());
        has_changes = true;
    }

    if old.width != new.width {
        delta.width = Some(new.width);
        has_changes = true;
    }

    if old.is_nullable != new.is_nullable {
        delta.is_nullable = Some(new.is_nullable);
        has_changes = true;
    }

    if old.primary_key != new.primary_key {
        delta.primary_key = Some(new.primary_key);
        has_changes = true;
    }

    if old.unique != new.unique {
        delta.unique = Some(new.unique);
        has_changes = true;
    }

    if old.unique_group != new.unique_group {
        delta.unique_group = Some(new.unique_group.clone());
        has_changes = true;
    }

    if old.indexed != new.indexed {
        delta.indexed = Some(new.indexed);
        has_changes = true;
    }

    if old.index_type != new.index_type {
        delta.index_type = Some(new.index_type.clone());
        has_changes = true;
    }

    if old.default != new.default {
        delta.default = Some(new.default.clone());
        has_changes = true;
    }

    if old.check != new.check {
        delta.check = Some(new.check.clone());
        has_changes = true;
    }

    if old.foreign_key != new.foreign_key {
        delta.foreign_key = Some(new.foreign_key.clone());
        has_changes = true;
    }

    if has_changes {
        Some(delta)
    } else {
        None
    }
}

/// Extract FK dependencies from table. Returns referenced table qualified names.
fn extract_fk_deps(table: &TableModel) -> Vec<String> {
    table.columns.iter()
        .filter_map(|col| col.foreign_key.as_ref())
        .map(|fk| {
            // Build qualified name for FK reference
            // Assume same schema if FK doesn't specify schema
            if fk.table.schema().is_some() {
                fk.table.to_string()
            } else {
                match table.qualified_name().schema() {
                    Some(schema) => format!("{}.{}", schema, fk.table.name()),
                    None => fk.table.to_string(),
                }
            }
        })
        .collect()
}

/// Wrapper for CreateTable actions to implement TopoNode
struct TableNode<'a> {
    action: &'a Action,
    name: String,
    deps: Vec<String>,
}

impl<'a> TopoNode for TableNode<'a> {
    fn key(&self) -> &str {
        &self.name
    }
    
    fn deps(&self) -> impl Iterator<Item = &str> {
        self.deps.iter().map(|s| s.as_str())
    }
}

/// Sort CreateTable actions by FK dependencies using topological sort.
/// Only considers dependencies within the batch being created.
/// Returns Err if circular dependency detected.
fn toposort_create_tables(actions: Vec<Action>) -> Result<Vec<Action>, StateError> {
    if actions.is_empty() {
        return Ok(actions);
    }
    
    // Build nodes with extracted dependencies
    let nodes: Vec<TableNode> = actions.iter()
        .map(|action| {
            if let Action::CreateTable { model } = action {
                TableNode {
                    action,
                    name: model.qualified_name().to_string(),
                    deps: extract_fk_deps(model),
                }
            } else {
                // Shouldn't happen, but handle gracefully
                TableNode {
                    action,
                    name: String::new(),
                    deps: vec![],
                }
            }
        })
        .collect();
    
    // Get sorted indices
    let indices = toposort_indices(&nodes)
        .map_err(|e| match e {
            TopoError::Cycle(names) => StateError::CircularFKDependency(
                names.first().cloned().unwrap_or_default()
            ),
            TopoError::DuplicateKey(name) => StateError::TableExists(name),
        })?;
    
    // Reorder actions by indices
    Ok(indices.into_iter()
        .map(|i| nodes[i].action.clone())
        .collect())
}

/// Sort DropTable actions in reverse FK dependency order.
fn toposort_drop_tables(actions: Vec<Action>, tables: &IndexMap<QualifiedName, Arc<TableModel>>) -> Result<Vec<Action>, StateError> {
    // Convert to CreateTable format for sorting (reuse toposort logic)
    let mut create_actions = Vec::with_capacity(actions.len());
    
    for action in &actions {
        if let Action::DropTable { name } = action {
            let table = tables.get(name)
                .ok_or_else(|| StateError::TableNotFound(name.to_string()))?;
            create_actions.push(Action::CreateTable {
                model: Arc::clone(table),
            });
        }
    }
    
    // Sort as if creating, then reverse
    let mut sorted = toposort_create_tables(create_actions)?;
    sorted.reverse();
    
    // Convert back to DropTable actions
    let drop_actions = sorted.iter()
        .filter_map(|action| {
            if let Action::CreateTable { model } = action {
                Some(Action::DropTable {
                    name: model.qualified_name().clone(),
                })
            } else {
                None
            }
        })
        .collect();
    
    Ok(drop_actions)
}

/// Wrapper for CreateOpaque/ReplaceOpaque actions to implement TopoNode
struct OpaqueNode<'a> {
    action: &'a Action,
    name: String,
    deps: Vec<String>,
}

impl<'a> TopoNode for OpaqueNode<'a> {
    fn key(&self) -> &str {
        &self.name
    }
    
    fn deps(&self) -> impl Iterator<Item = &str> {
        self.deps.iter().map(|s| s.as_str())
    }
}

/// Sort CreateOpaque/ReplaceOpaque actions by dependencies (tables + other opaques).
/// Only considers opaque-to-opaque dependencies within the batch.
/// Table dependencies are ignored (assumed to already exist or be created before opaques).
/// Returns Err if circular dependency detected.
fn toposort_create_opaques(
    actions: Vec<Action>,
    _existing_tables: &IndexMap<QualifiedName, Arc<TableModel>>,
) -> Result<Vec<Action>, StateError> {
    if actions.is_empty() {
        return Ok(actions);
    }
    
    // Build nodes with extracted dependencies
    // Note: depends_on_tables is intentionally ignored - table ordering is handled
    // by diff_from placing CreateTable actions before CreateOpaque actions
    let nodes: Vec<OpaqueNode> = actions.iter()
        .map(|action| {
            match action {
                Action::CreateOpaque { model } | Action::ReplaceOpaque { model } => {
                    OpaqueNode {
                        action,
                        name: model.qualified_name().to_string(),
                        deps: model.depends_on_opaques.iter().map(|qn| qn.to_string()).collect(),
                    }
                }
                _ => {
                    OpaqueNode {
                        action,
                        name: String::new(),
                        deps: vec![],
                    }
                }
            }
        })
        .collect();
    
    // Get sorted indices
    let indices = toposort_indices(&nodes)
        .map_err(|e| match e {
            TopoError::Cycle(names) => StateError::CircularOpaqueDependency(
                names.first().cloned().unwrap_or_default()
            ),
            TopoError::DuplicateKey(name) => StateError::OpaqueExists(name),
        })?;
    
    // Reorder actions by indices
    Ok(indices.into_iter()
        .map(|i| nodes[i].action.clone())
        .collect())
}

/// Sort DropOpaque actions in reverse dependency order.
fn toposort_drop_opaques(
    actions: Vec<Action>,
    old_opaques: &IndexMap<(OpaqueType, QualifiedName), Arc<OpaqueModel>>,
) -> Result<Vec<Action>, StateError> {
    // Convert to CreateOpaque format for sorting
    let create_actions: Vec<Action> = actions.iter()
        .filter_map(|action| {
            if let Action::DropOpaque { name, kind } = action {
                let key = (kind.clone(), name.clone());
                old_opaques.get(&key).map(|opaque| Action::CreateOpaque {
                    model: Arc::clone(opaque),
                })
            } else {
                None
            }
        })
        .collect();
    
    // Sort as if creating, then reverse
    let mut sorted = toposort_create_opaques(create_actions, &IndexMap::new())?;
    sorted.reverse();
    
    // Convert back to DropOpaque actions
    let drop_actions = sorted.iter()
        .filter_map(|action| {
            if let Action::CreateOpaque { model } = action {
                Some(Action::DropOpaque {
                    name: model.qualified_name().clone(),
                    kind: model.kind.clone(),
                })
            } else {
                None
            }
        })
        .collect();
    
    Ok(drop_actions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ColumnModel;

    fn make_table(name: &str, columns: Vec<&str>) -> TableModel {
        let mut table = TableModel::new(Arc::from(name), None);
        for col_name in columns {
            table.add_column(ColumnModel::new(Arc::from(col_name), Arc::from("TEXT")));
        }
        table
    }

    #[test]
    fn test_empty_state() {
        let state = MigState::new();
        assert_eq!(state.tables.len(), 0);
        assert_eq!(state.opaques.len(), 0);
    }

    #[test]
    fn test_add_table() {
        let mut state = MigState::new();
        let table = make_table("users", vec!["id", "name"]);
        state.add_table(table);
        assert_eq!(state.tables.len(), 1);
        assert!(state.get_table(&QualifiedName::parse(&Arc::from("users"))).is_some());
    }

    #[test]
    fn test_add_table_with_schema() {
        let mut state = MigState::new();
        let table = TableModel::new(Arc::from("users"), Some(Arc::from("public")));
        state.add_table(table);
        assert_eq!(state.tables.len(), 1);
        // Must use qualified name for lookup
        assert!(state.get_table(&QualifiedName::parse(&Arc::from("public.users"))).is_some());
        assert!(state.get_table(&QualifiedName::parse(&Arc::from("users"))).is_none());
    }

    #[test]
    fn test_remove_table() {
        let mut state = MigState::new();
        state.add_table(make_table("users", vec!["id"]));
        let removed = state.remove_table(&QualifiedName::parse(&Arc::from("users")));
        assert!(removed.is_some());
        assert_eq!(state.tables.len(), 0);
    }

    #[test]
    fn test_remove_table_with_schema() {
        let mut state = MigState::new();
        let table = TableModel::new(Arc::from("users"), Some(Arc::from("myschema")));
        state.add_table(table);
        // Must use qualified name for removal
        let removed = state.remove_table(&QualifiedName::parse(&Arc::from("myschema.users")));
        assert!(removed.is_some());
        assert_eq!(state.tables.len(), 0);
    }

    #[test]
    fn test_diff_empty_states() {
        let old_state = MigState::new();
        let new_state = MigState::new();
        let actions = new_state.diff_from(&old_state).unwrap();
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn test_diff_new_table() {
        let old_state = MigState::new();
        let mut new_state = MigState::new();
        new_state.add_table(make_table("users", vec!["id", "name"]));
        
        let actions = new_state.diff_from(&old_state).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::CreateTable { model } => assert_eq!(model.name(), "users"),
            _ => panic!("Expected CreateTable action"),
        }
    }

    #[test]
    fn test_diff_dropped_table() {
        let mut old_state = MigState::new();
        old_state.add_table(make_table("users", vec!["id"]));
        let new_state = MigState::new();
        
        let actions = new_state.diff_from(&old_state).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::DropTable { name, .. } => assert_eq!(name.to_string(), "users"),
            _ => panic!("Expected DropTable action"),
        }
    }

    #[test]
    fn test_apply_create_table() {
        let mut state = MigState::new();
        let table = make_table("users", vec!["id"]);
        let action = Action::CreateTable { model: Arc::new(table) };
        
        let result = state.apply_action(&action);
        assert!(result.is_ok());
        assert_eq!(state.tables.len(), 1);
    }

    #[test]
    fn test_apply_create_table_duplicate_error() {
        let mut state = MigState::new();
        state.add_table(make_table("users", vec!["id"]));
        
        let table = make_table("users", vec!["id", "name"]);
        let action = Action::CreateTable { model: Arc::new(table) };
        
        let result = state.apply_action(&action);
        assert!(result.is_err());
        match result.unwrap_err() {
            StateError::TableExists(name) => assert_eq!(name, "users"),
            _ => panic!("Expected TableExists error"),
        }
    }

    #[test]
    fn test_apply_drop_table() {
        let mut state = MigState::new();
        state.add_table(make_table("users", vec!["id"]));
        
        let action = Action::DropTable { 
            name: QualifiedName::parse("users") 
        };
        let result = state.apply_action(&action);
        assert!(result.is_ok());
        assert_eq!(state.tables.len(), 0);
    }

    #[test]
    fn test_apply_drop_table_not_found() {
        let mut state = MigState::new();
        let action = Action::DropTable { 
            name: QualifiedName::parse("users") 
        };
        
        let result = state.apply_action(&action);
        assert!(result.is_err());
        match result.unwrap_err() {
            StateError::TableNotFound(name) => assert_eq!(name, "users"),
            _ => panic!("Expected TableNotFound error"),
        }
    }

    #[test]
    fn test_apply_actions_with_schema() {
        let mut state = MigState::new();
        let table = TableModel::new(Arc::from("posts"), Some(Arc::from("public")));
        let action = Action::CreateTable { model: Arc::new(table) };
        
        state.apply_action(&action).unwrap();
        assert!(state.get_table(&QualifiedName::parse("public.posts")).is_some());

        // Add column to schema-qualified table
        let col_action = Action::CreateColumn {
            table: QualifiedName::parse("public.posts"),
            model: Arc::new(ColumnModel::new(Arc::from("title"), Arc::from("TEXT"))),
        };
        state.apply_action(&col_action).unwrap();
        
        let table_arc = state.get_table(&QualifiedName::parse("public.posts")).unwrap();
        assert_eq!(table_arc.columns.len(), 1);
    }

    #[test]
    fn test_diff_column_changes() {
        let mut old_state = MigState::new();
        old_state.add_table(make_table("users", vec!["id", "name"]));
        
        let mut new_state = MigState::new();
        new_state.add_table(make_table("users", vec!["id", "name", "email"]));
        
        let actions = new_state.diff_from(&old_state).unwrap();
        // Should detect column addition
        assert!(!actions.is_empty());
    }

    #[test]
    fn test_column_delta_computation() {
        let mut old_state = MigState::new();
        let mut old_table = TableModel::new(Arc::from("users"), None);
        let mut old_col = ColumnModel::new(Arc::from("age"), Arc::from("INTEGER"));
        old_col.is_nullable = true;
        old_table.add_column(old_col);
        old_state.add_table(old_table);

        let mut new_state = MigState::new();
        let mut new_table = TableModel::new(Arc::from("users"), None);
        let mut new_col = ColumnModel::new(Arc::from("age"), Arc::from("BIGINT"));
        new_col.is_nullable = false;
        new_col.width = Some(64);
        new_table.add_column(new_col);
        new_state.add_table(new_table);

        let actions = new_state.diff_from(&old_state).unwrap();
        assert_eq!(actions.len(), 1);
        
        match &actions[0] {
            Action::AlterColumn { table, name, delta } => {
                assert_eq!(table.to_string(), "users");
                assert_eq!(name.as_ref(), "age");
                assert_eq!(delta.data_type.as_ref().map(|s| s.as_ref()), Some("BIGINT"));
                assert_eq!(delta.is_nullable, Some(false));
                assert_eq!(delta.width, Some(Some(64)));
            }
            _ => panic!("Expected AlterColumn action"),
        }
    }

    #[test]
    fn test_column_rename_detection() {
        let mut old_state = MigState::new();
        let mut old_table = TableModel::new(Arc::from("users"), None);
        old_table.add_column(ColumnModel::new(Arc::from("old_name"), Arc::from("TEXT")));
        old_state.add_table(old_table);

        let mut new_state = MigState::new();
        let mut new_table = TableModel::new(Arc::from("users"), None);
        new_table.add_column(ColumnModel::new(Arc::from("new_name"), Arc::from("TEXT")));
        new_state.add_table(new_table);

        let actions = new_state.diff_from(&old_state).unwrap();
        // Name difference detected as drop + create (not rename, since we match by name)
        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0], Action::DropColumn { .. }));
        assert!(matches!(actions[1], Action::CreateColumn { .. }));
    }

    #[test]
    fn test_diff_with_schema_qualified_tables() {
        let mut old_state = MigState::new();
        let old_table = TableModel::new(Arc::from("users"), Some(Arc::from("public")));
        old_state.add_table(old_table);

        let mut new_state = MigState::new();
        let mut new_table = TableModel::new(Arc::from("users"), Some(Arc::from("public")));
        new_table.add_column(ColumnModel::new(Arc::from("id"), Arc::from("INTEGER")));
        new_state.add_table(new_table);

        let actions = new_state.diff_from(&old_state).unwrap();
        // Should detect column addition on qualified table
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::CreateColumn { table, model } => {
                assert_eq!(table.to_string(), "public.users");
                assert_eq!(model.name.as_ref(), "id");
            }
            _ => panic!("Expected CreateColumn action"),
        }
    }

    #[test]
    fn test_schema_isolation() {
        let mut state = MigState::new();
        let table1 = TableModel::new(Arc::from("users"), Some(Arc::from("schema1")));
        let table2 = TableModel::new(Arc::from("users"), Some(Arc::from("schema2")));
        let table3 = TableModel::new(Arc::from("users"), None);

        state.add_table(table1);
        state.add_table(table2);
        state.add_table(table3);

        // All three should coexist with different qualified names
        assert_eq!(state.tables.len(), 3);
        assert!(state.get_table(&QualifiedName::parse(&Arc::from("schema1.users"))).is_some());
        assert!(state.get_table(&QualifiedName::parse(&Arc::from("schema2.users"))).is_some());
        assert!(state.get_table(&QualifiedName::parse(&Arc::from("users"))).is_some());
    }

    #[test]
    fn test_action_ordering_safety() {
        use crate::db::models::OpaqueType;
        
        let mut old_state = MigState::new();
        
        // Old state: table with column, and opaque depending on it
        let mut old_table = TableModel::new(Arc::from("users"), None);
        old_table.add_column(ColumnModel::new(Arc::from("id"), Arc::from("INTEGER")));
        old_table.add_column(ColumnModel::new(Arc::from("old_col"), Arc::from("TEXT")));
        old_state.add_table(old_table);
        
        let old_opaque = OpaqueModel {
            qualified_name: QualifiedName::parse(&Arc::from("user_view")),
            kind: OpaqueType::View,
            definition: Arc::from("SELECT * FROM users"),
            related_table: None,
            depends_on_tables: vec![],
            depends_on_opaques: vec![],
        };
        old_state.add_opaque(old_opaque);
        
        // New state: table modified (column dropped, column added), opaque removed
        let mut new_table = TableModel::new(Arc::from("users"), None);
        new_table.add_column(ColumnModel::new(Arc::from("id"), Arc::from("INTEGER")));
        new_table.add_column(ColumnModel::new(Arc::from("new_col"), Arc::from("TEXT")));
        
        let mut new_state = MigState::new();
        new_state.add_table(new_table);
        
        let actions = new_state.diff_from(&old_state).unwrap();
        
        // Verify ordering: DropOpaque must come before DropColumn
        let drop_opaque_pos = actions.iter().position(|a| matches!(a, Action::DropOpaque { .. }));
        let drop_column_pos = actions.iter().position(|a| matches!(a, Action::DropColumn { .. }));
        let create_column_pos = actions.iter().position(|a| matches!(a, Action::CreateColumn { .. }));
        
        assert!(drop_opaque_pos.is_some(), "Should have DropOpaque");
        assert!(drop_column_pos.is_some(), "Should have DropColumn");
        assert!(create_column_pos.is_some(), "Should have CreateColumn");
        
        // DropOpaque before DropColumn
        let drop_opaque_idx = drop_opaque_pos.expect("verified above");
        let drop_column_idx = drop_column_pos.expect("verified above");
        let create_column_idx = create_column_pos.expect("verified above");
        
        assert!(drop_opaque_idx < drop_column_idx, 
            "DropOpaque must come before DropColumn");
        
        // DropColumn before CreateColumn
        assert!(drop_column_idx < create_column_idx,
            "DropColumn must come before CreateColumn");
    }

    #[test]
    fn test_action_ordering_create_drop() {
        use crate::db::models::OpaqueType;
        
        let mut old_state = MigState::new();
        old_state.add_table(make_table("old_table", vec!["id"]));
        
        let mut new_state = MigState::new();
        new_state.add_table(make_table("new_table", vec!["id"]));
        
        let new_opaque = OpaqueModel {
            qualified_name: QualifiedName::parse(&Arc::from("new_view")),
            kind: OpaqueType::View,
            definition: Arc::from("SELECT * FROM new_table"),
            related_table: None,
            depends_on_tables: vec![],
            depends_on_opaques: vec![],
        };
        new_state.add_opaque(new_opaque);
        
        let actions = new_state.diff_from(&old_state).unwrap();
        
        // Verify: DropTable before CreateTable before CreateOpaque
        let drop_table_pos = actions.iter().position(|a| matches!(a, Action::DropTable { .. }));
        let create_table_pos = actions.iter().position(|a| matches!(a, Action::CreateTable { .. }));
        let create_opaque_pos = actions.iter().position(|a| matches!(a, Action::CreateOpaque { .. }));
        
        assert!(drop_table_pos.is_some(), "Should have DropTable");
        assert!(create_table_pos.is_some(), "Should have CreateTable");
        assert!(create_opaque_pos.is_some(), "Should have CreateOpaque");
        
        let drop_idx = drop_table_pos.expect("verified above");
        let create_idx = create_table_pos.expect("verified above");
        let opaque_idx = create_opaque_pos.expect("verified above");
        
        assert!(drop_idx < create_idx,
            "DropTable before CreateTable");
        assert!(create_idx < opaque_idx,
            "CreateTable before CreateOpaque");
    }

    #[test]
    fn test_create_column_duplicate_error() {
        let mut state = MigState::new();
        let mut table = TableModel::new(Arc::from("users"), None);
        table.add_column(ColumnModel::new(Arc::from("id"), Arc::from("INTEGER")));
        state.add_table(table);

        // Try to create duplicate column
        let action = Action::CreateColumn {
            table: QualifiedName::parse("users"),
            model: Arc::new(ColumnModel::new(Arc::from("id"), Arc::from("TEXT"))),
        };

        let result = state.apply_action(&action);
        assert!(result.is_err());
        match result.unwrap_err() {
            StateError::DuplicateColumn { table, column } => {
                assert_eq!(table, "users");
                assert_eq!(column, "id");
            }
            _ => panic!("Expected DuplicateColumn error"),
        }
    }

    #[test]
    fn test_drop_column_not_found_error() {
        let mut state = MigState::new();
        let table = TableModel::new(Arc::from("users"), None);
        state.add_table(table);

        // Try to drop non-existent column
        let action = Action::DropColumn {
            table: QualifiedName::parse("users"),
            name: Arc::from("nonexistent"),
        };

        let result = state.apply_action(&action);
        assert!(result.is_err());
        match result.unwrap_err() {
            StateError::ColumnNotFound(name) => {
                assert_eq!(name, "nonexistent");
            }
            _ => panic!("Expected ColumnNotFound error"),
        }
    }

    #[test]
    fn test_create_column_success() {
        let mut state = MigState::new();
        state.add_table(TableModel::new(Arc::from("users"), None));

        let action = Action::CreateColumn {
            table: QualifiedName::parse("users"),
            model: Arc::new(ColumnModel::new(Arc::from("name"), Arc::from("TEXT"))),
        };

        let result = state.apply_action(&action);
        assert!(result.is_ok());
        
        let table = state.get_table(&QualifiedName::parse("users")).unwrap();
        assert_eq!(table.columns.len(), 1);
        assert_eq!(table.columns[0].name.as_ref(), "name");
    }

    #[test]
    fn test_drop_column_success() {
        let mut state = MigState::new();
        let mut table = TableModel::new(Arc::from("users"), None);
        table.add_column(ColumnModel::new(Arc::from("old_col"), Arc::from("TEXT")));
        state.add_table(table);

        let action = Action::DropColumn {
            table: QualifiedName::parse("users"),
            name: Arc::from("old_col"),
        };

        let result = state.apply_action(&action);
        assert!(result.is_ok());
        
        let table = state.get_table(&QualifiedName::parse("users")).unwrap();
        assert_eq!(table.columns.len(), 0);
    }

    #[test]
    fn test_fk_dependency_create_order() {
        use crate::db::models::ForeignKey;
        
        let old_state = MigState::new();
        let mut new_state = MigState::new();
        
        // Create posts table with FK to users
        let mut users_table = TableModel::new(Arc::from("users"), None);
        users_table.add_column(ColumnModel::new(Arc::from("id"), Arc::from("INTEGER")));
        
        let mut posts_table = TableModel::new(Arc::from("posts"), None);
        posts_table.add_column(ColumnModel::new(Arc::from("id"), Arc::from("INTEGER")));
        let mut user_id_col = ColumnModel::new(Arc::from("user_id"), Arc::from("INTEGER"));
        user_id_col.foreign_key = Some(ForeignKey {
            table: QualifiedName::parse(&Arc::from("users")),
            column: Arc::from("id"),
            name: None,
            on_delete: None,
        });
        posts_table.add_column(user_id_col);
        
        new_state.add_table(posts_table);
        new_state.add_table(users_table);
        
        let actions = new_state.diff_from(&old_state).unwrap();
        
        // Find CreateTable positions
        let users_pos = actions.iter().position(|a| {
            matches!(a, Action::CreateTable { model } if model.name() == "users")
        });
        let posts_pos = actions.iter().position(|a| {
            matches!(a, Action::CreateTable { model } if model.name() == "posts")
        });
        
        assert!(users_pos.is_some());
        assert!(posts_pos.is_some());
        // Users must be created before posts (FK dependency)
        assert!(users_pos.unwrap() < posts_pos.unwrap(), 
            "Referenced table 'users' must be created before 'posts'");
    }

    #[test]
    fn test_fk_dependency_drop_order() {
        use crate::db::models::ForeignKey;
        
        let mut old_state = MigState::new();
        
        // Old state: users and posts with FK
        let mut users_table = TableModel::new(Arc::from("users"), None);
        users_table.add_column(ColumnModel::new(Arc::from("id"), Arc::from("INTEGER")));
        
        let mut posts_table = TableModel::new(Arc::from("posts"), None);
        posts_table.add_column(ColumnModel::new(Arc::from("id"), Arc::from("INTEGER")));
        let mut user_id_col = ColumnModel::new(Arc::from("user_id"), Arc::from("INTEGER"));
        user_id_col.foreign_key = Some(ForeignKey {
            table: QualifiedName::parse(&Arc::from("users")),
            column: Arc::from("id"),
            name: None,
            on_delete: None,
        });
        posts_table.add_column(user_id_col);
        
        old_state.add_table(users_table);
        old_state.add_table(posts_table);
        
        let new_state = MigState::new();
        let actions = new_state.diff_from(&old_state).unwrap();
        
        // Find DropTable positions
        let users_pos = actions.iter().position(|a| {
            matches!(a, Action::DropTable { name, .. } if name.to_string() == "users")
        });
        let posts_pos = actions.iter().position(|a| {
            matches!(a, Action::DropTable { name, .. } if name.to_string() == "posts")
        });
        
        assert!(users_pos.is_some());
        assert!(posts_pos.is_some());
        // Posts must be dropped before users (reverse FK dependency)
        assert!(posts_pos.unwrap() < users_pos.unwrap(),
            "Referencing table 'posts' must be dropped before 'users'");
    }

    #[test]
    fn test_extended_column_delta() {
        let mut old_state = MigState::new();
        let mut old_table = TableModel::new(Arc::from("users"), None);
        let mut old_col = ColumnModel::new(Arc::from("status"), Arc::from("INTEGER"));
        old_col.is_nullable = true;
        old_col.default = Some(Arc::from("0"));
        old_table.add_column(old_col);
        old_state.add_table(old_table);

        let mut new_state = MigState::new();
        let mut new_table = TableModel::new(Arc::from("users"), None);
        let mut new_col = ColumnModel::new(Arc::from("status"), Arc::from("INTEGER"));
        new_col.is_nullable = false;
        new_col.default = Some(Arc::from("1"));
        new_col.indexed = true;
        new_table.add_column(new_col);
        new_state.add_table(new_table);

        let actions = new_state.diff_from(&old_state).unwrap();
        assert_eq!(actions.len(), 1);
        
        match &actions[0] {
            Action::AlterColumn { table, name, delta } => {
                assert_eq!(table.to_string(), "users");
                assert_eq!(name.as_ref(), "status");
                assert_eq!(delta.is_nullable, Some(false));
                assert_eq!(delta.default.as_ref().map(|opt| opt.as_ref().map(|s| s.as_ref())), Some(Some("1")));
                assert_eq!(delta.indexed, Some(true));
            }
            _ => panic!("Expected AlterColumn action"),
        }
    }

    #[test]
    fn test_opaque_dependency_ordering() {
        use crate::db::models::OpaqueType;
        
        let old_state = MigState::new();
        let mut new_state = MigState::new();
        
        // Create users table
        let mut users_table = TableModel::new(Arc::from("users"), None);
        users_table.add_column(ColumnModel::new(Arc::from("id"), Arc::from("INTEGER")));
        users_table.add_column(ColumnModel::new(Arc::from("active"), Arc::from("BOOLEAN")));
        new_state.add_table(users_table);
        
        // View depending on table
        let user_view = OpaqueModel {
            qualified_name: QualifiedName::parse(&Arc::from("active_users")),
            kind: OpaqueType::View,
            definition: Arc::from("SELECT * FROM users WHERE active"),
            related_table: None,
            depends_on_tables: vec![QualifiedName::parse(&Arc::from("users"))],
            depends_on_opaques: vec![],
        };
        
        // View depending on another view
        let summary_view = OpaqueModel {
            qualified_name: QualifiedName::parse(&Arc::from("user_summary")),
            kind: OpaqueType::View,
            definition: Arc::from("SELECT COUNT(*) FROM active_users"),
            related_table: None,
            depends_on_tables: vec![],
            depends_on_opaques: vec![QualifiedName::parse(&Arc::from("active_users"))],
        };
        
        // Add in reverse order to test sorting
        new_state.add_opaque(summary_view);
        new_state.add_opaque(user_view);
        
        let actions = new_state.diff_from(&old_state).unwrap();
        
        // Find positions
        let table_pos = actions.iter().position(|a| {
            matches!(a, Action::CreateTable { model } if model.name() == "users")
        });
        let view1_pos = actions.iter().position(|a| {
            matches!(a, Action::CreateOpaque { model } if model.qualified_name().name() == "active_users")
        });
        let view2_pos = actions.iter().position(|a| {
            matches!(a, Action::CreateOpaque { model } if model.qualified_name().name() == "user_summary")
        });
        
        assert!(table_pos.is_some(), "Table should be created");
        assert!(view1_pos.is_some(), "First view should be created");
        assert!(view2_pos.is_some(), "Second view should be created");
        
        // Table before dependent view
        assert!(table_pos.unwrap() < view1_pos.unwrap(),
            "Table 'users' must be created before dependent view 'active_users'");
        
        // First view before dependent view
        assert!(view1_pos.unwrap() < view2_pos.unwrap(),
            "View 'active_users' must be created before dependent view 'user_summary'");
    }

    #[test]
    fn test_opaque_dependency_drop_order() {
        use crate::db::models::OpaqueType;
        
        let mut old_state = MigState::new();
        
        // Create users table
        let mut users_table = TableModel::new(Arc::from("users"), None);
        users_table.add_column(ColumnModel::new(Arc::from("id"), Arc::from("INTEGER")));
        old_state.add_table(users_table);
        
        // View depending on table
        let user_view = OpaqueModel {
            qualified_name: QualifiedName::parse(&Arc::from("active_users")),
            kind: OpaqueType::View,
            definition: Arc::from("SELECT * FROM users WHERE active"),
            related_table: None,
            depends_on_tables: vec![QualifiedName::parse(&Arc::from("users"))],
            depends_on_opaques: vec![],
        };
        
        // View depending on another view
        let summary_view = OpaqueModel {
            qualified_name: QualifiedName::parse(&Arc::from("user_summary")),
            kind: OpaqueType::View,
            definition: Arc::from("SELECT COUNT(*) FROM active_users"),
            related_table: None,
            depends_on_tables: vec![],
            depends_on_opaques: vec![QualifiedName::parse(&Arc::from("active_users"))],
        };
        
        old_state.add_opaque(user_view);
        old_state.add_opaque(summary_view);
        
        let new_state = MigState::new();
        let actions = new_state.diff_from(&old_state).unwrap();
        
        // Find positions
        let view2_pos = actions.iter().position(|a| {
            matches!(a, Action::DropOpaque { name, .. } if name.to_string() == "user_summary")
        });
        let view1_pos = actions.iter().position(|a| {
            matches!(a, Action::DropOpaque { name, .. } if name.to_string() == "active_users")
        });
        let table_pos = actions.iter().position(|a| {
            matches!(a, Action::DropTable { name, .. } if name.to_string() == "users")
        });
        
        assert!(view2_pos.is_some(), "Dependent view should be dropped");
        assert!(view1_pos.is_some(), "First view should be dropped");
        assert!(table_pos.is_some(), "Table should be dropped");
        
        // Dependent view dropped before its dependency
        assert!(view2_pos.unwrap() < view1_pos.unwrap(),
            "View 'user_summary' must be dropped before 'active_users'");
        
        // Views dropped before table
        assert!(view1_pos.unwrap() < table_pos.unwrap(),
            "View 'active_users' must be dropped before table 'users'");
    }
}
