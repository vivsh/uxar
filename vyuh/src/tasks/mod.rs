mod backends;
pub(crate) mod store;
pub(crate) mod tasks;

pub use backends::memstore::MemoryTaskStore;
#[cfg(feature = "mysql")]
pub use backends::mysqlstore::MySqlTaskStore;
#[cfg(feature = "postgres")]
pub use backends::pgstore::PgTaskStore;
#[cfg(feature = "sqlite")]
pub use backends::sqlitestore::SqliteTaskStore;
pub use store::{AbstractTaskRunner, AbstractTaskStore};
pub use tasks::*;

#[cfg(feature = "postgres")]
pub type TaskStore = PgTaskStore;
#[cfg(feature = "mysql")]
pub type TaskStore = MySqlTaskStore;
#[cfg(feature = "sqlite")]
pub type TaskStore = SqliteTaskStore;
#[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
pub type TaskRunner = AbstractTaskRunner<TaskStore>;
