use proc_macro::TokenStream;
use quote::quote;

use darling::FromMeta;
use syn::{
    spanned::Spanned, Attribute, DeriveInput, Error, Expr, Fields, Lit, LitInt, LitStr, Meta,
    Result, Token, Type,
};
use syn::punctuated::Punctuated;

// -------------------------------------------------------------------------------------
// Namespace key allowlists (used only for better error messages; does not change acceptance)
// -------------------------------------------------------------------------------------

static VALIDATE_KEYS: &[&str] = &[
    "enum_values",
    "min_length",
    "max_length",
    "exact_length",
    "pattern",
    "email",
    "url",
    "uuid",
    "phone_e164",
    "ipv4",
    "ipv6",
    "date",
    "datetime",
    "min",
    "max",
    "exclusive_min",
    "exclusive_max",
    "multiple_of",
    "min_items",
    "max_items",
    "unique_items",
    "custom",
    "delegate",
];

static FIELD_KEYS: &[&str] = &["skip", "flatten", "json", "reference"];

static COLUMN_KEYS: &[&str] = &[
    "name",
    "primary_key",
    "serial",
    "skip",
    "flatten",
    "json",
    "reference",
    "selectable",
    "insertable",
    "updatable",
    "default",
    "index",
    "index_type",
    "unique",
    "unique_group",
];

// -------------------------------------------------------------------------------------
// Small helpers
// -------------------------------------------------------------------------------------

fn parse_ns_list(
    attr: &Attribute,
    ns: &str,
) -> Result<Option<Vec<darling::ast::NestedMeta>>> {
    if !attr.path().is_ident(ns) {
        return Ok(None);
    }

    let nested = match &attr.meta {
        syn::Meta::List(list) => {
            darling::ast::NestedMeta::parse_meta_list(list.tokens.clone())
                .map_err(|e| augment_darling_error(e.into(), &format!("Error parsing #[{ns}(...)]")))? // preserves spans
        }
        syn::Meta::Path(_) => Vec::new(),
        _ => {
            return Err(Error::new(
                attr.span(),
                format!("expected #[{ns}] or #[{ns}(...)]"),
            ))
        }
    };

    Ok(Some(nested))
}

fn top_level_key(nm: &darling::ast::NestedMeta) -> Option<&syn::Path> {
    use darling::ast::NestedMeta;
    match nm {
        NestedMeta::Meta(syn::Meta::Path(p)) => Some(p),
        NestedMeta::Meta(syn::Meta::NameValue(nv)) => Some(&nv.path),
        NestedMeta::Meta(syn::Meta::List(ml)) => Some(&ml.path),
        NestedMeta::Lit(_) => None,
    }
}

fn enforce_namespace(ns: &str, nested: &[darling::ast::NestedMeta], allowed: &[&str]) -> Result<()> {
    for nm in nested {
        let Some(path) = top_level_key(nm) else { continue };
        let Some(ident) = path.get_ident() else { continue };
        let key = ident.to_string();

        if !allowed.iter().any(|a| *a == key) {
            // "belongs to ..." hint (best-effort)
            let hint = if VALIDATE_KEYS.contains(&key.as_str()) {
                " (belongs to #[validate(...)] )"
            } else if COLUMN_KEYS.contains(&key.as_str()) {
                " (belongs to #[column(...)] )"
            } else if FIELD_KEYS.contains(&key.as_str()) {
                " (belongs to #[field(...)] )"
            } else {
                ""
            };

            return Err(Error::new(
                ident.span(),
                format!("Unknown option '{key}' in #[{ns}(...)]{hint}"),
            ));
        }
    }
    Ok(())
}

fn parse_enum_attribute(meta: &Meta) -> darling::Result<Vec<Lit>> {
    match meta {
        Meta::List(list) => {
            let nested = list
                .parse_args_with(Punctuated::<Lit, Token![,]>::parse_terminated)
                .map_err(darling::Error::from)?;
            Ok(nested.into_iter().collect())
        }
        _ => Err(darling::Error::custom("Expected list or array for enum").with_span(meta)),
    }
}

#[derive(Debug, Clone, Default)]
pub struct EnumWrapper(pub Vec<Lit>);

impl FromMeta for EnumWrapper {
    fn from_meta(item: &Meta) -> darling::Result<Self> {
        parse_enum_attribute(item).map(EnumWrapper)
    }
}



// -------------------------------------------------------------------------------------
// Attributes
// -------------------------------------------------------------------------------------

/// Container-level schema attributes
#[derive(Debug, Default, Clone, FromMeta)]
pub struct ContainerAttrs {
    #[darling(default)]
    pub table: Option<LitStr>,
    #[darling(default, rename = "tags")]
    pub tags: Vec<LitStr>,
}

/// Field-level validation attributes from #[validate(...)]
#[derive(Debug, Default, Clone, FromMeta)]
pub struct ValidateAttrs {
    #[darling(default, rename = "enum_values")]
    pub enumeration: EnumWrapper,

    #[darling(default)]
    pub min_length: Option<LitInt>,
    #[darling(default)]
    pub max_length: Option<LitInt>,
    #[darling(default)]
    pub exact_length: Option<LitInt>,

    #[darling(default)]
    pub pattern: Option<LitStr>,

    #[darling(default)]
    pub email: bool,
    #[darling(default)]
    pub url: bool,
    #[darling(default)]
    pub uuid: bool,
    #[darling(default)]
    pub phone_e164: bool,
    #[darling(default)]
    pub ipv4: bool,
    #[darling(default)]
    pub ipv6: bool,
    #[darling(default)]
    pub date: bool,
    #[darling(default)]
    pub datetime: bool,

    #[darling(default)]
    pub min: Option<LitInt>,
    #[darling(default)]
    pub max: Option<LitInt>,
    #[darling(default)]
    pub exclusive_min: bool,
    #[darling(default)]
    pub exclusive_max: bool,
    #[darling(default)]
    pub multiple_of: Option<LitInt>,

    #[darling(default)]
    pub min_items: Option<LitInt>,
    #[darling(default)]
    pub max_items: Option<LitInt>,
    #[darling(default)]
    pub unique_items: bool,

    #[darling(default)]
    pub custom: Option<syn::Path>,
    #[darling(default)]
    pub delegate: bool,
}

impl ValidateAttrs {
    pub fn validate(&self) -> Result<()> {
        // Enforce mutually exclusive delegate and custom
        if self.delegate && self.custom.is_some() {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                "Cannot use both delegate and custom on the same field",
            ));
        }

        // Enforce mutually exclusive format flags
        let format_flags = [
            ("email", self.email),
            ("url", self.url),
            ("uuid", self.uuid),
            ("phone_e164", self.phone_e164),
            ("ipv4", self.ipv4),
            ("ipv6", self.ipv6),
            ("date", self.date),
            ("datetime", self.datetime),
        ];
        let active: Vec<&str> = format_flags
            .iter()
            .filter(|(_, v)| *v)
            .map(|(n, _)| *n)
            .collect();
        if active.len() > 1 {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!("Cannot mix format validators: found {}", active.join(", ")),
            ));
        }

        // Enforce exclusive_min requires min
        if self.exclusive_min && self.min.is_none() {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                "exclusive_min requires min to be set",
            ));
        }

        // Enforce exclusive_max requires max
        if self.exclusive_max && self.max.is_none() {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                "exclusive_max requires max to be set",
            ));
        }

        // Enforce exact_length is exclusive with min/max_length
        if self.exact_length.is_some() && (self.min_length.is_some() || self.max_length.is_some())
        {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                "exact_length cannot be used with min_length or max_length",
            ));
        }

        Ok(())
    }
}

/// Field-level metadata from #[field(...)]
#[derive(Debug, Default, Clone, FromMeta)]
pub struct FieldAttrs {
    #[darling(default)]
    pub skip: bool,
    #[darling(default)]
    pub flatten: bool,
    #[darling(default)]
    pub json: bool,
    #[darling(default)]
    pub reference: bool,
}

impl FieldAttrs {
    pub fn validate(&self) -> Result<()> {
        // Enforce skip and flatten are mutually exclusive
        if self.skip && self.flatten {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                "Cannot use both skip and flatten on the same field",
            ));
        }
        Ok(())
    }
}

/// Database column metadata from #[column(...)]
#[derive(Debug, Default, Clone, FromMeta)]
pub struct ColumnAttrs {
    #[darling(default)]
    pub name: Option<LitStr>,

    #[darling(default)]
    pub primary_key: bool,
    #[darling(default)]
    pub serial: bool,

    #[darling(default)]
    pub skip: bool,
    #[darling(default)]
    pub flatten: bool,
    #[darling(default)]
    pub json: bool,
    #[darling(default)]
    pub reference: bool,

    #[darling(default)]
    pub selectable: Option<bool>,
    #[darling(default)]
    pub insertable: Option<bool>,
    #[darling(default)]
    pub updatable: Option<bool>,

    #[darling(default)]
    pub default: Option<LitStr>,

    #[darling(default)]
    pub index: bool,
    #[darling(default)]
    pub index_type: Option<LitStr>,

    #[darling(default)]
    pub unique: bool,

    #[darling(default, multiple, rename = "unique_group")]
    pub unique_groups: Vec<LitStr>,
}

/// Combined field attributes across all namespaces
#[derive(Debug, Clone)]
pub struct FieldMeta {
    pub ident: Option<syn::Ident>,
    pub ty: Type,
    pub validate: ValidateAttrs,
    pub field: FieldAttrs,
    pub column: ColumnAttrs,
}

fn augment_darling_error(e: darling::Error, context: &str) -> syn::Error {
    let mut combined_err: Option<syn::Error> = None;

    for single_err in e {
        let syn_err: syn::Error = single_err.into();
        let msg = format!("{}: {}", context, syn_err);
        let enriched = Error::new(syn_err.span(), msg);
        match combined_err {
            Some(ref mut existing) => existing.combine(enriched),
            None => combined_err = Some(enriched),
        }
    }

    combined_err.unwrap_or_else(|| {
        Error::new(
            proc_macro2::Span::call_site(),
            format!("{}: Unknown error", context),
        )
    })
}

impl FieldMeta {
    pub fn from_field(field: &syn::Field) -> Result<Self> {
        let mut validate = ValidateAttrs::default();
        let mut field_attrs = FieldAttrs::default();
        let mut column = ColumnAttrs::default();

        let field_name = field
            .ident
            .as_ref()
            .map(|i| i.to_string())
            .unwrap_or_else(|| "<unnamed>".to_string());

        for attr in &field.attrs {
            if let Some(nested) = parse_ns_list(attr, "validate")? {
                enforce_namespace("validate", &nested, VALIDATE_KEYS)?;
                validate = ValidateAttrs::from_list(&nested).map_err(|e| {
                    augment_darling_error(e, &format!("Error decoding #[validate] on field '{field_name}'"))
                })?;
                validate
                    .validate()
                    // keep exact same message semantics; just pin to this attribute's span
                    .map_err(|e| Error::new(attr.span(), e.to_string()))?;
            }

            if let Some(nested) = parse_ns_list(attr, "field")? {
                enforce_namespace("field", &nested, FIELD_KEYS)?;
                field_attrs = FieldAttrs::from_list(&nested).map_err(|e| {
                    augment_darling_error(e, &format!("Error decoding #[field] on field '{field_name}'"))
                })?;
                field_attrs
                    .validate()
                    .map_err(|e| Error::new(attr.span(), e.to_string()))?;
            }

            if let Some(nested) = parse_ns_list(attr, "column")? {
                enforce_namespace("column", &nested, COLUMN_KEYS)?;
                column = ColumnAttrs::from_list(&nested).map_err(|e| {
                    augment_darling_error(e, &format!("Error decoding #[column] on field '{field_name}'"))
                })?;
            }
        }

        Ok(Self {
            ident: field.ident.clone(),
            ty: field.ty.clone(),
            validate,
            field: field_attrs,
            column,
        })
    }
}

impl ContainerAttrs {
    pub fn from_attrs(attrs: &[Attribute]) -> Result<Self> {
        for attr in attrs {
            if let Some(nested) = parse_ns_list(attr, "schema")? {
                // no allowlist here because schema is tiny; keep behavior and avoid extra maintenance
                return ContainerAttrs::from_list(&nested)
                    .map_err(|e| augment_darling_error(e, "Error decoding #[schema]"));
            }
        }
        Ok(Self::default())
    }
}

/// Parsed struct information ready for codegen
#[derive(Debug)]
pub struct ParsedStruct {
    pub ident: syn::Ident,
    pub generics: syn::Generics,
    pub container: ContainerAttrs,
    pub fields: Vec<FieldMeta>,
}

impl ParsedStruct {
    pub fn from_derive_input(input: DeriveInput) -> Result<Self> {
        let container = ContainerAttrs::from_attrs(&input.attrs)?;
        let ident = input.ident;
        let generics = input.generics;

        let fields = match input.data {
            syn::Data::Struct(data) => match data.fields {
                Fields::Named(fields) => fields
                    .named
                    .iter()
                    .map(FieldMeta::from_field)
                    .collect::<Result<Vec<_>>>()?,
                _ => {
                    return Err(Error::new_spanned(
                        ident,
                        "only structs with named fields are supported",
                    ))
                }
            },
            _ => return Err(Error::new_spanned(ident, "only structs are supported")),
        };

        Ok(Self {
            ident,
            generics,
            container,
            fields,
        })
    }
}

/// Extract doc comments from attributes
pub fn extract_doc_comment(attrs: &[Attribute]) -> Option<String> {
    let mut docs = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let Ok(meta) = attr.meta.require_name_value() {
                if let syn::Expr::Lit(expr_lit) = &meta.value {
                    if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                        docs.push(lit_str.value());
                    }
                }
            }
        }
    }
    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n").trim().to_string())
    }
}

pub fn derive_schemable_impl(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);

    match derive_schemable_inner(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn derive_schemable_inner(input: DeriveInput) -> Result<proc_macro2::TokenStream> {
    let input_attrs = input.attrs.clone();

    let raw_fields = if let syn::Data::Struct(ref data) = input.data {
        if let syn::Fields::Named(ref fields) = data.fields {
            fields.named.iter().cloned().collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let parsed = ParsedStruct::from_derive_input(input)?;
    let ident = &parsed.ident;
    let (impl_generics, ty_generics, where_clause) = parsed.generics.split_for_impl();

    let schema_name = ident.to_string();

    let table_name_expr = gen_table_name(&parsed.container.table);

    let about_expr = gen_about(&input_attrs);

    let tags: Vec<_> = parsed.container.tags.iter().map(|s| s.value()).collect();
    let tags_expr = quote! { ::std::vec![#(::std::borrow::Cow::Borrowed(#tags)),*] };

    // Generate field definitions
    let mut field_defs = Vec::with_capacity(parsed.fields.len());

    for (idx, field_meta) in parsed.fields.iter().enumerate() {
        let field_ident = match &field_meta.ident {
            Some(id) => id,
            None => return Err(Error::new(
                proc_macro2::Span::call_site(),
                "only named fields are supported",
            )),
        };
        let field_name = field_ident.to_string();
        let ty = &field_meta.ty;

        // Record nature from field attributes (just what's there, no logic)
        let field_nature = gen_nature(&field_meta.field);

        let field_skip = gen_skip(field_meta.field.skip);

        let field_about = if let Some(f) = raw_fields.get(idx) {
            if let Some(doc) = extract_doc_comment(&f.attrs) {
                quote! { Some(::std::borrow::Cow::Borrowed(#doc)) }
            } else {
                quote! { None }
            }
        } else {
            quote! { None }
        };

        let schema_type_expr = quote! { <#ty as ::uxar::schemables::Schemable>::schema_type() };

        // Build constraints - enumeration
        let enumeration = gen_enumeration(&field_meta.validate.enumeration, field_meta.ident.as_ref())?;

        let min_length = lit_int_to_tokens(field_meta.validate.min_length.as_ref());
        let max_length = lit_int_to_tokens(field_meta.validate.max_length.as_ref());
        let exact_length = lit_int_to_tokens(field_meta.validate.exact_length.as_ref());

        let pattern = gen_pattern(&field_meta.validate.pattern);

        let format = gen_format(&field_meta.validate);

        let minimum = parse_scalar_int(&field_meta.validate.min)?;

        let maximum = parse_scalar_int(&field_meta.validate.max)?;

        let exclusive_minimum = field_meta.validate.exclusive_min;
        let exclusive_maximum = field_meta.validate.exclusive_max;

        let multiple_of = parse_scalar_int(&field_meta.validate.multiple_of)?;

        let min_items = lit_int_to_tokens(field_meta.validate.min_items.as_ref());
        let max_items = lit_int_to_tokens(field_meta.validate.max_items.as_ref());
        let unique_items = field_meta.validate.unique_items;

        // Build column meta
        let col_name = gen_col_name(&field_meta.column.name);

        let primary_key = field_meta.column.primary_key;
        let serial = field_meta.column.serial;

        let col_default = gen_col_default(&field_meta.column.default);

        let index = field_meta.column.index;

        let index_type = gen_index_type(&field_meta.column.index_type);

        let unique = field_meta.column.unique;

        let unique_groups = gen_unique_groups(&field_meta.column.unique_groups);

        // Record column nature (just what's in column attrs)
        let col_nature = gen_col_nature(&field_meta.column);

        let col_skip = gen_skip(field_meta.column.skip);

        field_defs.push(quote! {
            ::uxar::schemables::SchemaField {
                name: ::std::borrow::Cow::Borrowed(#field_name),
                about: #field_about,
                schema_type: #schema_type_expr,
                constraints: ::uxar::schemables::SchemaConstraints {
                    enumeration: #enumeration,
                    min_length: #min_length,
                    max_length: #max_length,
                    exact_length: #exact_length,
                    pattern: #pattern,
                    format: #format,
                    minimum: #minimum,
                    maximum: #maximum,
                    exclusive_minimum: #exclusive_minimum,
                    exclusive_maximum: #exclusive_maximum,
                    multiple_of: #multiple_of,
                    min_items: #min_items,
                    max_items: #max_items,
                    unique_items: #unique_items,
                },
                skip: #field_skip,
                nature: #field_nature,
                column_meta: ::uxar::schemables::ColumnMeta {
                    name: #col_name,
                    primary_key: #primary_key,
                    serial: #serial,
                    skip: #col_skip,
                    nature: #col_nature,
                    default: #col_default,
                    index: #index,
                    index_type: #index_type,
                    unique: #unique,
                    unique_groups: #unique_groups,
                },
            }
        });
    }

    let expanded = quote! {
        impl #impl_generics ::uxar::schemables::Schemable for #ident #ty_generics #where_clause {
            fn schema_type() -> ::uxar::schemables::SchemaType {
                ::uxar::schemables::SchemaType::Struct(::uxar::schemables::StructSchema {
                    name: ::std::borrow::Cow::Borrowed(#schema_name),
                    fields: ::std::vec![#(#field_defs),*],
                    about: #about_expr,
                    table: ::uxar::schemables::TableMeta {
                        name: #table_name_expr,
                    },
                    tags: #tags_expr,
                })
            }
        }
    };

    Ok(expanded)
}

/// Generate table name expression.
fn gen_table_name(table: &Option<LitStr>) -> proc_macro2::TokenStream {
    if let Some(t) = table {
        let val = t.value();
        quote! { Some(::std::borrow::Cow::Borrowed(#val)) }
    } else {
        quote! { None }
    }
}

/// Generate about expression from doc comments.
fn gen_about(attrs: &[Attribute]) -> proc_macro2::TokenStream {
    if let Some(doc) = extract_doc_comment(attrs) {
        quote! { Some(::std::borrow::Cow::Borrowed(#doc)) }
    } else {
        quote! { None }
    }
}

/// Generate tags expression.
fn gen_tags(tags: &[LitStr]) -> proc_macro2::TokenStream {
    let tag_vals: Vec<_> = tags.iter().map(|s| s.value()).collect();
    quote! { ::std::vec![#(::std::borrow::Cow::Borrowed(#tag_vals)),*] }
}

/// Generate field nature from field attributes.
fn gen_nature(field: &FieldAttrs) -> proc_macro2::TokenStream {
    if field.flatten {
        quote! { Some(::uxar::schemables::Nature::Flatten) }
    } else if field.json {
        quote! { Some(::uxar::schemables::Nature::Json) }
    } else if field.reference {
        quote! { Some(::uxar::schemables::Nature::Reference) }
    } else {
        quote! { None }
    }
}

/// Generate skip flag.
fn gen_skip(skip: bool) -> proc_macro2::TokenStream {
    if skip {
        quote! { Some(true) }
    } else {
        quote! { None }
    }
}

/// Parse scalar int safely.
fn parse_scalar_int(lit: &Option<LitInt>) -> Result<proc_macro2::TokenStream> {
    if let Some(v) = lit {
        let val = v.base10_parse::<i128>()
            .map_err(|e| Error::new_spanned(v, format!("Failed to parse integer: {}", e)))?;
        Ok(quote! { Some(::uxar::schemables::ScalarLit::Int(#val)) })
    } else {
        Ok(quote! { None })
    }
}

/// Generate pattern expression.
fn gen_pattern(pattern: &Option<LitStr>) -> proc_macro2::TokenStream {
    if let Some(s) = pattern {
        let val = s.value();
        quote! { Some(::std::borrow::Cow::Borrowed(#val)) }
    } else {
        quote! { None }
    }
}

/// Generate string format from validation.
fn gen_format(validate: &ValidateAttrs) -> proc_macro2::TokenStream {
    if validate.email {
        quote! { Some(::uxar::schemables::StringFormat::Email) }
    } else if validate.url {
        quote! { Some(::uxar::schemables::StringFormat::Url) }
    } else if validate.uuid {
        quote! { Some(::uxar::schemables::StringFormat::Uuid) }
    } else if validate.phone_e164 {
        quote! { Some(::uxar::schemables::StringFormat::PhoneE164) }
    } else if validate.ipv4 {
        quote! { Some(::uxar::schemables::StringFormat::IpV4) }
    } else if validate.ipv6 {
        quote! { Some(::uxar::schemables::StringFormat::IpV6) }
    } else if validate.date {
        quote! { Some(::uxar::schemables::StringFormat::Date) }
    } else if validate.datetime {
        quote! { Some(::uxar::schemables::StringFormat::DateTime) }
    } else {
        quote! { None }
    }
}

/// Generate enumeration constraint with type check.
fn gen_enumeration(wrapper: &EnumWrapper, ident: Option<&syn::Ident>) -> Result<proc_macro2::TokenStream> {
    if wrapper.0.is_empty() {
        return Ok(quote! { ::std::vec![] });
    }

    let enums = &wrapper.0;
    let first_type = lit_discriminant(&enums[0]);
    
    for (idx, lit) in enums.iter().enumerate().skip(1) {
        let lit_type = lit_discriminant(lit);
        if lit_type != first_type {
            return Err(Error::new_spanned(
                ident,
                format!(
                    "enumeration values must be homogeneous; found {} at position 0 and {} at position {}",
                    first_type, lit_type, idx
                ),
            ));
        }
    }

    let enum_tokens: Vec<_> = enums.iter().map(lit_to_scalar_lit_token).collect();
    Ok(quote! { ::std::vec![#(#enum_tokens),*] })
}

/// Get discriminant for Lit type checking.
fn lit_discriminant(lit: &Lit) -> &'static str {
    match lit {
        Lit::Str(_) => "string",
        Lit::Int(_) => "integer",
        Lit::Float(_) => "float",
        Lit::Bool(_) => "boolean",
        Lit::Byte(_) => "byte",
        Lit::Char(_) => "char",
        _ => "other",
    }
}

/// Helper: Convert Lit to ScalarLit token for enumeration
fn lit_to_scalar_lit_token(lit: &Lit) -> proc_macro2::TokenStream {
    match lit {
        Lit::Str(s) => {
            let val = s.value();
            quote! { ::uxar::schemables::ScalarLit::Str(#val) }
        }
        Lit::Int(i) => {
            if let Ok(val) = i.base10_parse::<i128>() {
                quote! { ::uxar::schemables::ScalarLit::Int(#val) }
            } else {
                quote! { ::uxar::schemables::ScalarLit::Int(0) }
            }
        }
        Lit::Float(f) => {
            if let Ok(val) = f.base10_parse::<f64>() {
                quote! { ::uxar::schemables::ScalarLit::Float(#val) }
            } else {
                quote! { ::uxar::schemables::ScalarLit::Float(0.0) }
            }
        }
        Lit::Bool(b) => {
            let val = b.value;
            quote! { ::uxar::schemables::ScalarLit::Bool(#val) }
        }
        Lit::Byte(b) => {
            let val = b.value() as i128;
            quote! { ::uxar::schemables::ScalarLit::Int(#val) }
        }
        Lit::Char(c) => {
            let val = c.value().to_string();
            quote! { ::uxar::schemables::ScalarLit::Str(#val) }
        }
        _ => quote! { ::uxar::schemables::ScalarLit::Str("") },
    }
}

/// Generate column name expression.
fn gen_col_name(name: &Option<LitStr>) -> proc_macro2::TokenStream {
    if let Some(n) = name {
        let val = n.value();
        quote! { Some(::std::borrow::Cow::Borrowed(#val)) }
    } else {
        quote! { None }
    }
}

/// Convert LitInt to token stream safely.
fn lit_int_to_tokens(lit: Option<&LitInt>) -> proc_macro2::TokenStream {
    match lit {
        Some(v) => {
            if let Ok(val) = v.base10_parse::<usize>() {
                quote! { Some(#val) }
            } else {
                quote! { None }
            }
        }
        None => quote! { None },
    }
}

/// Generate column default expression.
fn gen_col_default(default: &Option<LitStr>) -> proc_macro2::TokenStream {
    if let Some(s) = default {
        let val = s.value();
        quote! { Some(::std::borrow::Cow::Borrowed(#val)) }
    } else {
        quote! { None }
    }
}

/// Generate index type expression.
fn gen_index_type(index_type: &Option<LitStr>) -> proc_macro2::TokenStream {
    if let Some(s) = index_type {
        let val = s.value();
        quote! { Some(::std::borrow::Cow::Borrowed(#val)) }
    } else {
        quote! { None }
    }
}

/// Generate unique groups with capacity hint.
fn gen_unique_groups(groups: &[LitStr]) -> proc_macro2::TokenStream {
    if groups.is_empty() {
        quote! { ::std::vec![] }
    } else {
        let group_strs: Vec<_> = groups
            .iter()
            .map(|s| quote! { ::std::borrow::Cow::Borrowed(#s) })
            .collect();
        quote! { ::std::vec![#(#group_strs),*] }
    }
}

/// Generate column nature from column attributes.
fn gen_col_nature(col: &ColumnAttrs) -> proc_macro2::TokenStream {
    if col.flatten {
        quote! { Some(::uxar::schemables::Nature::Flatten) }
    } else if col.json {
        quote! { Some(::uxar::schemables::Nature::Json) }
    } else if col.reference {
        quote! { Some(::uxar::schemables::Nature::Reference) }
    } else {
        quote! { None }
    }
}

#[cfg(test)]
#[path = "schemable_tests.rs"]
mod tests;