
pub(crate) mod tasks;
pub(crate) mod store;
pub(crate) mod indentity;
mod backends;

pub use tasks::*;
pub use store::AbstractTaskRunner;
pub use backends::pgstore::PgTaskStore;


#[cfg(feature = "postgres")]
pub type TaskStore = PgTaskStore;
pub type TaskRunner = AbstractTaskRunner<TaskStore>;
#[cfg(not(feature = "postgres"))]
compile_error!("Postgres feature must be enabled to use TaskRunner");
