use darling::FromMeta;
use syn::{
    spanned::Spanned, Attribute, DeriveInput, Error, Fields, Lit, LitInt, LitStr, Meta,
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

/// Reference specification for foreign key relationships
#[derive(Debug, Clone, Default)]
pub struct ReferenceSpec {
    pub from: Option<String>,
    pub to: Option<String>,
}

impl FromMeta for ReferenceSpec {
    fn from_word() -> darling::Result<Self> {
        Ok(ReferenceSpec {
            from: None,
            to: None,
        })
    }
    
    fn from_list(items: &[darling::ast::NestedMeta]) -> darling::Result<Self> {
        #[derive(FromMeta)]
        struct RefParams {
            #[darling(default)]
            from: Option<LitStr>,
            #[darling(default)]
            to: Option<LitStr>,
        }
        
        let params = RefParams::from_list(items)?;
        Ok(ReferenceSpec {
            from: params.from.map(|lit| lit.value()),
            to: params.to.map(|lit| lit.value()),
        })
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
    pub reference: Option<ReferenceSpec>,

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
