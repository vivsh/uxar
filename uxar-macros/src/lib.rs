
mod schemable;
mod route;
mod bundle;
mod validate;
mod filterable;
mod bindable;
mod scannable;
mod bitrole;
mod cron;
mod periodic;
mod pgnotify;
mod signal;
mod task;
mod flow;
mod assets;
mod openapi;
mod bundlepart;
mod service;


use proc_macro::TokenStream;
extern crate proc_macro;


/// Derives the Validate trait for data validation.
/// 
/// Generates validation logic based on `#[validate(...)]` attributes.
/// 
/// # Attributes
/// 
/// ## `#[validate(...)]`
/// - `delegate` - Delegate validation to the field's type (must implement `Validate`)
/// - `custom = "path"` - Call a custom validation function: `fn(&T) -> Result<(), ValidationReport>`
/// - String: `min_length`, `max_length`, `exact_length`, `pattern`
/// - String formats: `email`, `url`, `uuid`, `phone_e164`, `ipv4`, `ipv6`
/// - Numeric: `min`, `max`, `exclusive_min`, `exclusive_max`, `multiple_of`
/// - Array: `min_items`, `max_items`, `unique_items`
#[proc_macro_derive(Validate, attributes(validate))]
pub fn derive_validate(input: TokenStream) -> TokenStream {
    validate::derive_validate_impl(input)
}



/// Defines a route handler with metadata for routing and OpenAPI documentation.
/// 
/// Can be applied to free functions or methods in impl blocks.
/// 
/// # Required Attributes
/// 
/// - `method` - HTTP method: `"get"`, `"post"`, `"put"`, `"patch"`, `"delete"`, `"head"`, `"options"`, or `"trace"`
/// - `url` - Path pattern with optional parameters in braces: `"/users/{id}"`
/// 
/// # Optional Attributes
/// 
/// - `tags` - Array of OpenAPI tags: `tags = ["users", "api"]`
/// - `name` - Route name for reverse routing (defaults to function name)
/// - `summary` - Short description for OpenAPI (defaults to first doc comment line)
/// - `description` - Detailed description for OpenAPI (defaults to remaining doc comments)
/// 
/// # Examples
/// 
/// ```ignore
/// // Free function
/// #[route(method = "get", url = "/users/{id}", tags = ["users"])]
/// async fn get_user(Path(id): Path<i32>) -> Json<User> {
///     // ...
/// }
/// 
/// // Method in impl block
/// impl UserApi {
///     #[route(method = "post", url = "/users", tags = ["users"])]
///     async fn create_user(Json(data): Json<CreateUser>) -> Json<User> {
///         // ...
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn route(attr: TokenStream, item: TokenStream) -> TokenStream {
    route::parse_route(attr, item)
}

/// Collects bundle parts (routes, tasks, signals) into a Bundle for composition and registration.
///
/// Bundles are the primary unit for organizing and composing application components.
/// Each handler must be annotated with appropriate macros (`#[route]`, `#[cron]`, `#[periodic]`, etc.).
/// 
/// # Syntax
/// 
/// ```ignore
/// bundle! {
///     handler1,
///     handler2,
///     ...,
///     tags = ["tag1", "tag2"]  // optional, applies only to routes
/// }
/// ```
/// 
/// # Options
/// 
/// - `tags` - Optional array of tags to apply to all routes in the bundle.
///            These tags extend (not replace) any tags defined on individual routes.
///            Note: tags only apply to route parts, not other bundle parts.
/// 
/// # Examples
/// 
/// ```ignore
/// // Bundle without tags
/// let user_bundle = bundle! {
///     get_user,        // #[route]
///     create_user,     // #[route]
///     sync_users,      // #[cron]
/// };
/// 
/// // Bundle with tags - extends individual route tags
/// let api_bundle = bundle! {
///     tags = ["api", "v1"],
///     get_user,
///     create_user,
/// };
/// 
/// // Compose bundles
/// let all_bundles = bundle! {
///     user_bundle,
///     api_bundle,
/// };
/// ```
/// 
/// # Notes
/// 
/// - Handlers must be annotated with `#[route]`, `#[cron]`, `#[periodic]`, `#[pgnotify]`, or `#[signal]`
/// - Handlers can be free functions or references to IntoBundle types
/// - Tags are additive and only apply to route parts
/// - Returns a `Bundle` that implements `IntoBundle`
#[proc_macro]
pub fn bundle(input: TokenStream) -> TokenStream {
    bundle::parse_bundle(input)
}


#[proc_macro_derive(Filterable, attributes(filterable, filter))]
pub fn derive_filterable(input: TokenStream) -> TokenStream {
    filterable::derive_filterable(input)
}

#[proc_macro_derive(Bindable, attributes(field, column))]
pub fn derive_bindable(input: TokenStream) -> TokenStream {
    bindable::derive_bindable(input)
}

#[proc_macro_derive(Scannable, attributes(field, column))]
pub fn derive_scannable(input: TokenStream) -> TokenStream {
    scannable::derive_scannable(input)
}

/// Derives the BitRole trait for role-based access control.
/// 
/// Automatically implements BitRole for enums with unit variants only.
/// Each variant is assigned a bit position (0, 1, 2, ...) for role masking.
/// 
/// # Requirements
/// - Only unit variants allowed (no tuple or struct variants)
/// - Explicit discriminants must be > 0
/// - Enum must derive Copy, Debug, and implement IntoEnumIterator (from strum)
/// 
/// # Example
/// ```ignore
/// #[derive(Debug, Copy, Clone, BitRole, EnumIter)]
/// enum UserRole {
///     Viewer = 1,
///     Editor = 2,
///     Admin = 3,
/// }
/// ```
#[proc_macro_derive(BitRole, attributes(bitrole))]
pub fn derive_bitrole(input: TokenStream) -> TokenStream {
    bitrole::derive_bitrole(input)
}

/// Schedules a function to run periodically based on a cron expression.
/// 
/// Annotated functions will be registered as cron jobs in the bundle.
/// The function must accept a `Site` parameter and return a type that can be
/// wrapped in `SignalPayload`.
/// 
/// # Attributes
/// 
/// - `expr` - Cron expression (required): `"0 0 * * *"` (daily at midnight)
/// 
/// # Examples
/// 
/// ```ignore
/// // Free function
/// #[cron(expr = "0 0 * * *")]
/// fn sync_daily(site: Site) -> SyncResult {
///     // runs daily at midnight
/// }
/// 
/// // Method in impl block
/// impl SyncTasks {
///     #[cron(expr = "*/5 * * * *")]
///     fn sync_frequent(site: Site) -> SyncResult {
///         // runs every 5 minutes
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn cron(attr: TokenStream, item: TokenStream) -> TokenStream {
    cron::parse_cron(attr, item)
}

/// Schedules a function to run periodically at fixed intervals.
/// 
/// Annotated functions will be registered as periodic tasks in the bundle.
/// The function must accept a `Site` parameter and return a type that can be
/// wrapped in `SignalPayload`.
/// 
/// # Attributes
/// 
/// - `secs` - Interval in seconds (optional)
/// - `millis` - Interval in milliseconds (optional)
/// 
/// At least one of `secs` or `millis` must be specified. Both can be used together.
/// 
/// # Examples
/// 
/// ```ignore
/// // Free function - runs every 30 seconds
/// #[periodic(secs = 30)]
/// fn health_check(site: Site) -> CheckResult {
///     // ...
/// }
/// 
/// // Method - runs every 500ms
/// impl Monitor {
///     #[periodic(millis = 500)]
///     fn monitor_metrics(site: Site) -> Metrics {
///         // ...
///     }
/// }
/// 
/// // Combined - runs every 1.5 seconds
/// #[periodic(secs = 1, millis = 500)]
/// fn poll_queue(site: Site) -> QueueStatus {
///     // ...
/// }
/// ```
#[proc_macro_attribute]
pub fn periodic(attr: TokenStream, item: TokenStream) -> TokenStream {
    periodic::parse_periodic(attr, item)
}

/// Registers a function as a PostgreSQL NOTIFY/LISTEN handler.
/// 
/// Annotated functions will listen for notifications on a PostgreSQL channel.
/// The function must accept a `&str` payload and return `Result<T, SignalError>`
/// where T can be wrapped in `SignalPayload`.
/// 
/// # Attributes
/// 
/// - `channel` - PostgreSQL channel name (required): `"user_updates"`
/// 
/// # Examples
/// 
/// ```ignore
/// // Free function
/// #[pgnotify(channel = "user_updates")]
/// fn handle_user_update(payload: &str) -> Result<UserUpdate, SignalError> {
///     serde_json::from_str(payload)
///         .map_err(|_| SignalError::PayloadTypeMismatch)
/// }
/// 
/// // Method in impl block
/// impl UserHandlers {
///     #[pgnotify(channel = "notifications")]
///     fn handle_notification(payload: &str) -> Result<Notification, SignalError> {
///         // parse and return notification
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn pgnotify(attr: TokenStream, item: TokenStream) -> TokenStream {
    pgnotify::parse_pgnotify(attr, item)
}

/// Registers a function as a generic signal handler.
/// 
/// Annotated functions will be registered to handle any signal events.
/// The function must accept `Site` and `Arc<dyn Any + Send + Sync>` parameters
/// and return a Future.
/// 
/// # Examples
/// 
/// ```ignore
/// // Free function
/// #[signal]
/// async fn handle_signal(site: Site, payload: Arc<dyn Any + Send + Sync>) {
///     // handle generic signal
/// }
/// 
/// // Method in impl block
/// impl SignalHandlers {
///     #[signal]
///     async fn process_event(site: Site, payload: Arc<dyn Any + Send + Sync>) {
///         // process event
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn signal(attr: TokenStream, item: TokenStream) -> TokenStream {
    signal::parse_signal(attr, item)
}

/// Registers a function as a unit task handler.
/// 
/// Unit tasks are async operations that execute once and complete. The function
/// must accept `Site` and a deserializable input type, returning a TaskUnitOutput.
/// 
/// # Attributes
/// 
/// - `name` - Optional task name (defaults to function name)
/// 
/// # Examples
/// 
/// ```ignore
/// // Free function with default name
/// #[task]
/// async fn send_email(site: Site, input: EmailData) -> Result<TaskUnitOutput, TaskError> {
///     // send email
/// }
/// 
/// // Method with custom name
/// impl TaskHandlers {
///     #[task(name = "custom_task_name")]
///     async fn process_order(site: Site, order: Order) -> Result<TaskUnitOutput, TaskError> {
///         // process order
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn task(attr: TokenStream, item: TokenStream) -> TokenStream {
    task::parse_task(attr, item)
}

/// Registers a function as a flow task handler.
/// 
/// Flow tasks are synchronous operations that can spawn child tasks. The function
/// accepts a deserializable input type and returns TaskFlowOutput.
/// 
/// # Attributes
/// 
/// - `name` - Optional task name (defaults to function name)
/// 
/// # Examples
/// 
/// ```ignore
/// // Free function with default name
/// #[flow]
/// fn process_batch(input: BatchData) -> Result<TaskFlowOutput, TaskError> {
///     // process and potentially spawn child tasks
/// }
/// 
/// // Method with custom name
/// impl FlowHandlers {
///     #[flow(name = "workflow_step")]
///     fn execute_workflow(data: WorkflowData) -> Result<TaskFlowOutput, TaskError> {
///         // execute workflow step
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn flow(attr: TokenStream, item: TokenStream) -> TokenStream {
    flow::parse_flow(attr, item)
}



// #[proc_macro_attribute]
// pub fn fnspec(attr: TokenStream, item: TokenStream) -> TokenStream {
//     fnspec::parse_fnspec_input(attr, item, "fnspec")
// }


#[proc_macro_attribute]
pub fn openapi(attr: TokenStream, item: TokenStream) -> TokenStream {
    openapi::parse_openapi(attr, item)
}


#[proc_macro_attribute]
pub fn service(attr: TokenStream, item: TokenStream) -> TokenStream {
    service::parse_service(attr, item)
}


#[proc_macro_attribute]
pub fn asset_dir(attr: TokenStream, item: TokenStream) -> TokenStream {
    assets::parse_asset_dir(attr, item)
}