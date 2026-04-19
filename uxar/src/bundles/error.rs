use std::sync::Arc;

use crate::signals::SignalError;

#[derive(Debug, thiserror::Error, Clone)]
pub enum BundleError {
    #[error(transparent)]
    Signal(#[from] Arc<SignalError>),

    #[error(transparent)]
    Task(#[from] Arc<crate::tasks::TaskError>),

    #[error(transparent)]
    Emitter(#[from] Arc<crate::emitters::EmitterError>),

    #[error(transparent)]
    Service(#[from] Arc<crate::services::ServiceError>),

    #[error(transparent)]
    Command(Arc<crate::commands::CommandError>),

    #[error("Multiple errors occurred: {0:?}")]
    ErrorList(Vec<BundleError>),

    #[error("API doc generation failed: {0}")]
    DocGen(String),
}
