//! Common imports for building Vyuh applications.
//!
//! Covers the six core subsystems — routes, tasks, signals, commands, services,
//! and channels — for both macro and macro-less registration styles:
//!
//! ```rust
//! use vyuh::prelude::*;
//! ```

// ── Core framework types ────────────────────────────────────────────────────

pub use crate::{Data, Error, Site, SiteConf, SiteError, Valid, Validate};

pub use schemars::JsonSchema;
pub use serde::{Deserialize, Serialize};

// ── Subsystem modules (needed for attribute macros: #[bundles::task], etc.) ─

pub use crate::{bundles, channels, commands, routes, services, signals, tasks};

// ── Bundle ──────────────────────────────────────────────────────────────────
// Types for function signatures + macro-less builder functions.

pub use crate::bundles::{Bundle, IntoBundle};
pub use crate::bundles::{bundle, command, route, service, signal, task};

// ── Routes ──────────────────────────────────────────────────────────────────
// Handler return types + HTTP method builders for macro-less registration.

pub use crate::routes::{
    Form, IntoResponse, Json, Path, Query, State, StatusCode,
    any, delete, get, patch, post, put,
    Methods, RouteConf,
};

// ── Tasks ───────────────────────────────────────────────────────────────────

pub use crate::tasks::{Suspension, TaskClient, TaskContext, TaskHandlerConf, TaskState};

// ── Signals ─────────────────────────────────────────────────────────────────

pub use crate::signals::{SignalClient, SignalError};

// ── Commands ─────────────────────────────────────────────────────────────────

pub use crate::commands::{CommandConf, CommandContext};

// ── Services ─────────────────────────────────────────────────────────────────

pub use crate::services::{Service, ServiceError, ServiceInstance, ServiceRunner};

// ── Channels ─────────────────────────────────────────────────────────────────

pub use crate::channels::{ChannelLongPoll, ChannelRef, ChannelSse, ChannelWebSocket};

// ── Task backend (feature-gated) ─────────────────────────────────────────────

#[cfg(feature = "mysql")]
pub use crate::tasks::{MySqlTaskStore, TaskRunner, TaskStore};

#[cfg(feature = "postgres")]
pub use crate::tasks::{PgTaskStore, TaskRunner, TaskStore};

#[cfg(feature = "sqlite")]
pub use crate::tasks::{SqliteTaskStore, TaskRunner, TaskStore};

#[cfg(not(any(feature = "postgres", feature = "mysql", feature = "sqlite")))]
pub use crate::tasks::{TaskRunner, TaskStore};
