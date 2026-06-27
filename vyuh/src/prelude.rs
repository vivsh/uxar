//! Common imports for ordinary Vyuh application code.
//!
//! The prelude contains symbols most handlers and examples need directly.
//! Registration APIs stay under the `bundles::` namespace:
//!
//! ```rust
//! use vyuh::prelude::*;
//!
//! #[bundles::route(path = "/health")]
//! async fn health() -> Json<&'static str> {
//!     Json("ok")
//! }
//! ```

// ── Core framework types ────────────────────────────────────────────────────

pub use crate::{Data, Error, Site, SiteConf, SiteError, Valid, Validate};

pub use schemars::JsonSchema;
pub use serde::{Deserialize, Serialize};

// ── Registration namespace ──────────────────────────────────────────────────

pub use crate::bundles;

// ── Routes ──────────────────────────────────────────────────────────────────

pub use crate::routes::{
    AppendHeaders, Form, Html, IntoResponse, Json, NoContent, POLL, Path, Query, Redirect, SSE,
    State, StatusCode, Subscriber, WS,
};

// ── Tasks ───────────────────────────────────────────────────────────────────

pub use crate::tasks::{Suspension, TaskOptions, TaskState};

// ── Services ─────────────────────────────────────────────────────────────────

pub use crate::services::ServiceRef;

// ── Channels ─────────────────────────────────────────────────────────────────

pub use crate::channels::{ChannelResponse, Channels, UserKey};
