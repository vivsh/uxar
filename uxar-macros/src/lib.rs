
mod schemable;
mod scannable;
mod validatable;
mod bindable;
mod filterable;
mod routable;
mod model;

use proc_macro::TokenStream;
extern crate proc_macro;


/// Derives database schema traits for a type.
/// 
/// Implements: SchemaInfo, Scannable, Bindable, and Validatable traits.
/// 
/// Use `#[derive(Validatable)]` if you only need validation without database operations.
/// 
/// # Attributes
/// 
/// ## `#[model(...)]` - Type-level attributes
/// - `name = "custom_name"` - Override the schema name (defaults to struct name)
/// - `db_table = "table_name"` - Optional database table name (for documentation/migration hints)
/// 
/// ## `#[field(...)]` - Field attributes (Django-style db_ prefix for DB constraints)
/// 
/// ### General field mapping
/// - `skip` - Exclude field from schema
/// - `flatten` - Flatten nested struct fields
/// - `json` - Store as JSON
/// - `reference` - Reference to another type
/// - `selectable = false` - Exclude from SELECT queries
/// - `insertable = false` - Exclude from INSERT queries  
/// - `updatable = false` - Exclude from UPDATE queries
/// 
/// ### Database constraints (optional, for migration generation)
/// - `db_column = "column_name"` - Database column name (defaults to field name)
/// - `primary_key` - Mark as primary key
/// - `unique` - Add UNIQUE constraint
/// - `unique_group = "group_name"` - Composite unique constraint
/// - `db_indexed` - Create database index
/// - `db_index_type = "btree"` - Index type (btree, hash, gin, etc.)
/// - `db_default = "value"` - Default value expression
/// - `db_check = "expression"` - CHECK constraint
/// 
/// ## `#[validate(...)]` - Validation attributes
/// - `email` - Validate email format
/// - `url` - Validate URL format
/// - `min_length = n` - Minimum string length
/// - `max_length = n` - Maximum string length
/// - `regex = "pattern"` - Regex pattern validation
/// - And more validation rules...
#[proc_macro_derive(Model, attributes(field, validate, model))]
pub fn derive_model(input: TokenStream) -> TokenStream {
    model::derive_model(input)
}


#[proc_macro_derive(Filterable, attributes(filterable, filter))]
pub fn derive_filterable(input: TokenStream) -> TokenStream {
    filterable::derive_filterable(input)
}


/// Derives validation trait for a type.
/// 
/// Use this for types that need validation but don't interact with the database.
/// Database models should use `#[derive(Schemable)]` which includes validation.
#[proc_macro_derive(Validatable, attributes(validate))]
pub fn derive_validatable(input: TokenStream) -> TokenStream {
    validatable::derive_validatable(input)
}


#[proc_macro_attribute]
pub fn route(attr: TokenStream, item: TokenStream) -> TokenStream {
    routable::parse_action(attr, item)
}

#[proc_macro_attribute]
pub fn routable(attr: TokenStream, item: TokenStream) -> TokenStream {
    routable::parse_routable(attr, item)
}