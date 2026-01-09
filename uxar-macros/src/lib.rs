
mod schemable;
mod route;
mod bundle;
mod validate;
mod filterable;
mod bindable;
mod scannable;
mod models;
mod bitrole;

use proc_macro::TokenStream;
extern crate proc_macro;



/// Derives the Schemable trait for unified type schema.
/// 
/// Generates static schema metadata for OpenAPI, database DDL, and migrations.
/// 
/// # Attributes
/// 
/// ## `#[schema(...)]` - Container attributes
/// - `table = "table_name"` - Database table name
/// - `tags("tag1", "tag2")` - Schema tags
/// 
/// Use doc comments (`///`) for descriptions on structs and fields.
/// 
/// ## `#[field(...)]` - Field metadata
/// - `skip` - Exclude from schema (can also be in #[column])
/// - `flatten` - Flatten nested struct (can also be in #[column])
/// - `json` - Store as JSON (can also be in #[column])
/// - `reference` - Reference to another type (can also be in #[column])
/// 
/// ## `#[validate(...)]` - Validation constraints
/// - String: `min_length`, `max_length`, `exact_length`, `pattern`
/// - String formats: `email`, `url`, `uuid`, `phone_e164`, `ipv4`, `ipv6`
/// - Numeric: `min`, `max`, `exclusive_min`, `exclusive_max`, `multiple_of`
/// - Array: `min_items`, `max_items`, `unique_items`
/// 
/// ## `#[column(...)]` - Database column metadata
/// - `name = "col_name"` - Column name override
/// - `primary_key` - Mark as primary key
/// - `serial` - Auto-increment column
/// - `skip` - Exclude from schema (can also be in #[field])
/// - `flatten` - Flatten nested struct (can also be in #[field])
/// - `json` - Store as JSON (can also be in #[field])
/// - `reference` - Reference to another type (can also be in #[field])
/// - `default = "value"` - Default value
/// - `index` - Create index
/// - `index_type = "btree"` - Index type
/// - `unique` - Unique constraint
/// - `unique_groups("group1", "group2")` - Composite unique constraints
#[proc_macro_derive(Schemable, attributes(schema, field, validate, column))]
pub fn derive_schemable(input: TokenStream) -> TokenStream {
    schemable::derive_schemable_impl(input)
}

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
    // Try to parse as free function first
    if let Ok(_) = syn::parse::<syn::ItemFn>(item.clone()) {
        route::parse_route_fn(attr, item)
    } else if let Ok(_) = syn::parse::<syn::ImplItemFn>(item.clone()) {
        // It's a method in an impl block
        route::parse_route_method(attr, item)
    } else {
        // Not a function or method - return error
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[route] can only be applied to functions or methods"
        )
        .to_compile_error()
        .into()
    }
}

/// Collects route handlers into a Bundle for composition and registration.
///
/// Bundles are the primary unit for organizing and composing routes in the application.
/// Each handler must be annotated with `#[route]` to provide routing metadata.
/// 
/// # Syntax
/// 
/// ```ignore
/// bundle_routes! {
///     handler1,
///     handler2,
///     ...,
///     tags = ["tag1", "tag2"]  // optional
/// }
/// ```
/// 
/// # Options
/// 
/// - `tags` - Optional array of tags to apply to all routes in the bundle.
///            These tags extend (not replace) any tags defined on individual routes.
/// 
/// # Examples
/// 
/// ```ignore
/// // Bundle without tags
/// let user_routes = bundle_routes! {
///     get_user,
///     create_user,
///     update_user,
/// };
/// 
/// // Bundle with tags - extends individual route tags
/// let api_routes = bundle_routes! {
///     tags = ["api", "v1"],
///     get_user,
///     create_user,
/// };
/// 
/// // Compose bundles
/// let all_routes = bundle_routes! {
///     user_routes,
///     api_routes,
/// };
/// ```
/// 
/// # Notes
/// 
/// - All handlers must be annotated with `#[route]` macro
/// - Handlers can be free functions or references to IntoBundle types
/// - Tags are additive - bundle-level tags extend route-level tags
/// - Returns a `Bundle` that implements `IntoBundle`
#[proc_macro]
pub fn bundle_routes(input: TokenStream) -> TokenStream {
    bundle::parse_bundle(input)
}

/// Implements IntoBundle for an impl block containing route handlers.
///
/// Automatically collects all methods annotated with `#[route]` and generates
/// an implementation of `IntoBundle` that registers them as a bundle.
/// 
/// # Syntax
/// 
/// ```ignore
/// #[bundle_impl(tags = ["tag1", "tag2"])]  // tags are optional
/// impl TypeName {
///     #[route(method = "get", url = "/path")]
///     async fn handler1() { }
///     
///     #[route(method = "post", url = "/path")]
///     async fn handler2() { }
/// }
/// ```
/// 
/// # Options
/// 
/// - `tags` - Optional array of tags to apply to all routes in this impl block.
///            These tags extend (not replace) any tags defined on individual routes.
/// 
/// # Examples
/// 
/// ```ignore
/// struct UserApi;
/// 
/// #[bundle_impl(tags = ["users", "api"])]
/// impl UserApi {
///     /// Get user by ID
///     #[route(method = "get", url = "/users/{id}")]
///     async fn get_user(Path(id): Path<i32>) -> Json<User> {
///         // ...
///     }
///     
///     /// Create a new user
///     #[route(method = "post", url = "/users")]
///     async fn create_user(Json(data): Json<CreateUser>) -> Json<User> {
///         // ...
///     }
/// }
/// 
/// // Usage
/// let bundle = UserApi.into_bundle();
/// ```
/// 
/// # Notes
/// 
/// - Only methods with `#[route]` are included in the bundle
/// - Non-route methods are ignored and remain as regular methods
/// - Tags are additive - bundle-level tags extend route-level tags
/// - Generates an `IntoBundle` implementation that can be called via `.into_bundle()`
#[proc_macro_attribute]
pub fn bundle_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    bundle::parse_bundle_attr(attr, item)
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

#[proc_macro_derive(Model, attributes(schema, field, validate, column))]
pub fn derive_model(input: TokenStream) -> TokenStream {
    models::derive_model(input)
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
