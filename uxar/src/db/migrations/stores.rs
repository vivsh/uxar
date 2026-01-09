use crate::db::migrations::actions::Action;

use super::errors::MigrationError;


/// Abstraction over the storage of applied migrations.
/// This allows different backends (DB table, file, etc) and handles forward/backward actions.
/// All operations for a single migration are always run in a single thread only.
pub trait MigrationStore {
    /// list of applied migration ids in order of application
    fn get_ordered_applied_id_list(
        &self,
    ) -> impl Future<Output = Result<Vec<String>, MigrationError>> + '_ + Send;

    /// begin the transaction context for the migration.
    fn begin(&self, migration_id: &str) -> impl Future<Output = Result<(), MigrationError>> + '_ + Send;

    /// run action in context of migration.
    /// Use migration_id for logging / error reporting .
    /// flush should be called at the end of the migration to persist the applied migration record.
    fn run_action(
        &self,
        migration_id: &str,
        action: &Action,
    ) -> impl Future<Output = Result<(), MigrationError>> + '_ + Send;

    /// commit the transaction and
    /// record the migration_id as applied in the store
    /// Store should remove any existing migration records after this migration_id.
    /// Use forward=true for applying migration, false for rolling back.
    fn commit(
        &self,
        migration_id: &str,
        forward: bool,
    ) -> impl Future<Output = Result<(), MigrationError>> + '_ + Send;

    fn rollback(
        &self,
        migration_id: &str,
    ) -> impl Future<Output = Result<(), MigrationError>> + '_ + Send;

}