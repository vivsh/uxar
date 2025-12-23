
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
/// If `table_name` is specified, also implements Recordable for migration support.
/// 
/// # Attributes
/// 
/// ## `#[schemable(...)]` - Type-level attributes
/// - `name = "custom_name"` - Override the schema name (defaults to struct name)
/// - `table_name = "table_name"` - Database table name (enables Recordable trait)
/// 
/// ## `#[column(...)]` - Field mapping attributes
/// - `db_column = "column_name"` - Database column name (defaults to field name)
/// - `skip` - Exclude field from schema
/// - `flatten` - Flatten nested struct fields
/// - `json` - Store as JSON
/// - `reference` - Reference to another type
/// - `selectable = false` - Exclude from SELECT queries
/// - `insertable = false` - Exclude from INSERT queries
/// - `updatable = false` - Exclude from UPDATE queries
/// - `primary_key` - Mark as primary key (also stored in ColumnSpec)
/// 
/// ## `#[db(...)]` - Database constraint attributes (only used with table_name)
/// - `primary_key` - Mark as primary key
/// - `unique` - Add UNIQUE constraint
/// - `unique_group = "group_name"` - Composite unique constraint
/// - `indexed` - Create database index
/// - `index_type = "btree"` - Index type (btree, hash, gin, etc.)
/// - `default = "value"` - Default value expression
/// - `check = "expression"` - CHECK constraint
/// 
/// Note: Currently, both `#[column(...)]` and `#[db(...)]` accept all attributes.
/// The recommendation is to use `#[column(...)]` for field mapping and `#[db(...)]` 
/// for database constraints, but they can be used interchangeably.
#[proc_macro_derive(Schemable, attributes(column, validate, schemable, db))]
pub fn derive_schemable(input: TokenStream) -> TokenStream {
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