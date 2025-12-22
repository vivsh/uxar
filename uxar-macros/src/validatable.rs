use darling::ast::Data;
use darling::{FromDeriveInput, FromField};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Lit};

/// Field-level validation configuration extracted from attributes.
#[derive(Debug, Clone, FromField)]
#[darling(attributes(validate))]
pub struct ValidatableField {
    pub ident: Option<syn::Ident>,
    pub ty: syn::Type,

    /// Skip validation for this field: #[validate(skip)]
    #[darling(default)]
    pub skip: bool,

    /// Custom validator function: #[validate(custom = "my_validator")]
    #[darling(default)]
    pub custom: Option<String>,

    /// Nested validation: #[validate(nested)]
    #[darling(default)]
    pub nested: bool,

    /// Email validation: #[validate(email)]
    #[darling(default)]
    pub email: bool,

    /// URL validation: #[validate(url)]
    #[darling(default)]
    pub url: bool,

    /// Minimum length: #[validate(min_length = 3)]
    #[darling(default)]
    pub min_length: Option<usize>,

    /// Maximum length: #[validate(max_length = 100)]
    #[darling(default)]
    pub max_length: Option<usize>,

    /// Minimum value: #[validate(min_value = 0)]
    #[darling(default)]
    pub min_value: Option<String>,

    /// Maximum value: #[validate(max_value = 100)]
    #[darling(default)]
    pub max_value: Option<String>,

    /// Non-empty string: #[validate(non_empty)]
    #[darling(default)]
    pub non_empty: bool,

    /// Required (for Option<T>): #[validate(required)]
    #[darling(default)]
    pub required: bool,

    /// Regex pattern: #[validate(regex = "^[0-9]+$")]
    #[darling(default)]
    pub regex: Option<String>,

    /// Alphanumeric: #[validate(alphanumeric)]
    #[darling(default)]
    pub alphanumeric: bool,

    /// Digits only: #[validate(digits)]
    #[darling(default)]
    pub digits: bool,

    /// UUID format: #[validate(uuid)]
    #[darling(default)]
    pub uuid: bool,
}

#[derive(FromDeriveInput)]
#[darling(attributes(validate), supports(struct_named))]
pub struct ValidatableInput {
    pub ident: syn::Ident,
    pub generics: syn::Generics,

    #[darling(default, rename = "crate")]
    pub crate_path: Option<syn::Path>,

    pub data: Data<darling::util::Ignored, ValidatableField>,
}

pub fn derive_validatable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let validatable = match ValidatableInput::from_derive_input(&input) {
        Ok(v) => v,
        Err(e) => return e.write_errors().into(),
    };

    impl_validatable(validatable).into()
}

// Helper to check if a type is Option<T>
fn is_option_type(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}

// Helper to generate validation code that works with both T and Option<T>
fn make_validation(
    is_option: bool,
    field_name: &syn::Ident,
    field_name_str: &str,
    crate_path: &syn::Path,
    validator_expr: TokenStream2,
) -> TokenStream2 {
    if is_option {
        quote! {
            if let Some(ref value) = self.#field_name {
                match #validator_expr {
                    Ok(()) => {},
                    Err(e) => report.push(
                        #crate_path::validation::Path::root().prefixed(
                            #crate_path::validation::PathSeg::Field(#field_name_str.into())
                        ),
                        e
                    ),
                }
            }
        }
    } else {
        quote! {
            match #validator_expr {
                Ok(()) => {},
                Err(e) => report.push(
                    #crate_path::validation::Path::root().prefixed(
                        #crate_path::validation::PathSeg::Field(#field_name_str.into())
                    ),
                    e
                ),
            }
        }
    }
}

/// Generate field validation code - can be reused for schema generation
pub fn generate_field_validation(
    field: &ValidatableField,
    field_name: &syn::Ident,
    crate_path: &syn::Path,
) -> Vec<TokenStream2> {
    let mut validations = Vec::new();

    if field.skip {
        return validations;
    }

    let field_name_str = field_name.to_string();
    let is_option = is_option_type(&field.ty);

    // Required (for Option<T>)
    if field.required {
        validations.push(quote! {
            match #crate_path::validation::present(&self.#field_name) {
                Ok(()) => {},
                Err(e) => report.push(
                    #crate_path::validation::Path::root().prefixed(
                        #crate_path::validation::PathSeg::Field(#field_name_str.into())
                    ),
                    e
                ),
            }
        });
    }

    // Nested validation
    if field.nested {
        let validation_code = if is_option {
            quote! {
                if let Some(ref value) = self.#field_name {
                    match value.validate() {
                        Ok(()) => {},
                        Err(errs) => report.extend(errs.at_field(#field_name_str)),
                    }
                }
            }
        } else {
            quote! {
                match self.#field_name.validate() {
                    Ok(()) => {},
                    Err(errs) => report.extend(errs.at_field(#field_name_str)),
                }
            }
        };
        validations.push(validation_code);
        return validations;
    }

    // Email
    if field.email {
        let value_ref = if is_option { quote! { value } } else { quote! { &self.#field_name } };
        let validator = quote! { #crate_path::validation::email(#value_ref) };
        validations.push(make_validation(is_option, field_name, &field_name_str, crate_path, validator));
    }

    // URL
    if field.url {
        let value_ref = if is_option { quote! { value } } else { quote! { &self.#field_name } };
        let validator = quote! { #crate_path::validation::url(#value_ref) };
        validations.push(make_validation(is_option, field_name, &field_name_str, crate_path, validator));
    }

    // Non-empty
    if field.non_empty {
        let value_ref = if is_option { quote! { value } } else { quote! { &self.#field_name } };
        let validator = quote! { #crate_path::validation::non_empty(#value_ref) };
        validations.push(make_validation(is_option, field_name, &field_name_str, crate_path, validator));
    }

    // Min length
    if let Some(min) = field.min_length {
        let value_ref = if is_option { quote! { value } } else { quote! { &self.#field_name } };
        let validator = quote! { #crate_path::validation::min_len(#min)(#value_ref) };
        validations.push(make_validation(is_option, field_name, &field_name_str, crate_path, validator));
    }

    // Max length
    if let Some(max) = field.max_length {
        let value_ref = if is_option { quote! { value } } else { quote! { &self.#field_name } };
        let validator = quote! { #crate_path::validation::max_len(#max)(#value_ref) };
        validations.push(make_validation(is_option, field_name, &field_name_str, crate_path, validator));
    }

    // Min value
    if let Some(ref min_str) = field.min_value {
        if let Ok(min_lit) = syn::parse_str::<Lit>(min_str) {
            let value_ref = if is_option { quote! { value } } else { quote! { &self.#field_name } };
            let validator = quote! { #crate_path::validation::min(#min_lit)(#value_ref) };
            validations.push(make_validation(is_option, field_name, &field_name_str, crate_path, validator));
        }
    }

    // Max value
    if let Some(ref max_str) = field.max_value {
        if let Ok(max_lit) = syn::parse_str::<Lit>(max_str) {
            let value_ref = if is_option { quote! { value } } else { quote! { &self.#field_name } };
            let validator = quote! { #crate_path::validation::max(#max_lit)(#value_ref) };
            validations.push(make_validation(is_option, field_name, &field_name_str, crate_path, validator));
        }
    }

    // Regex
    if let Some(ref pattern) = field.regex {
        let value_ref = if is_option { quote! { value } } else { quote! { &self.#field_name } };
        let validator = quote! {
            {
                static REGEX: once_cell::sync::Lazy<regex::Regex> = 
                    once_cell::sync::Lazy::new(|| {
                        regex::Regex::new(#pattern).expect("valid regex pattern")
                    });
                #crate_path::validation::regex(&*REGEX)(#value_ref)
            }
        };
        validations.push(make_validation(is_option, field_name, &field_name_str, crate_path, validator));
    }

    // Alphanumeric
    if field.alphanumeric {
        let value_ref = if is_option { quote! { value } } else { quote! { &self.#field_name } };
        let validator = quote! { #crate_path::validation::alphanumeric(#value_ref) };
        validations.push(make_validation(is_option, field_name, &field_name_str, crate_path, validator));
    }

    // Digits
    if field.digits {
        let value_ref = if is_option { quote! { value } } else { quote! { &self.#field_name } };
        let validator = quote! { #crate_path::validation::digits(#value_ref) };
        validations.push(make_validation(is_option, field_name, &field_name_str, crate_path, validator));
    }

    // UUID
    if field.uuid {
        let value_ref = if is_option { quote! { value } } else { quote! { &self.#field_name } };
        let validator = quote! { #crate_path::validation::uuid(#value_ref) };
        validations.push(make_validation(is_option, field_name, &field_name_str, crate_path, validator));
    }

    // Custom validator
    if let Some(ref custom_fn) = field.custom {
        let custom_ident = syn::parse_str::<syn::Path>(custom_fn)
            .expect("valid function path");
        let value_ref = if is_option { quote! { value } } else { quote! { &self.#field_name } };
        let validator = quote! { #custom_ident(#value_ref) };
        validations.push(make_validation(is_option, field_name, &field_name_str, crate_path, validator));
    }

    validations
}

fn impl_validatable(input: ValidatableInput) -> TokenStream2 {
    let ident = input.ident;
    let generics = input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let crate_path: syn::Path = input
        .crate_path
        .unwrap_or_else(|| syn::parse_quote!(uxar));

    let fields = match input.data {
        Data::Struct(s) => s.fields,
        _ => {
            return syn::Error::new_spanned(
                ident,
                "Validatable can only be derived for structs with named fields",
            )
            .to_compile_error()
        }
    };

    let mut all_validations = Vec::new();

    for field in fields {
        if let Some(field_name) = &field.ident {
            let validations = generate_field_validation(&field, field_name, &crate_path);
            all_validations.extend(validations);
        }
    }

    quote! {
        impl #impl_generics #crate_path::validation::Validate for #ident #ty_generics #where_clause {
            fn validate(&self) -> Result<(), #crate_path::validation::ValidationReport> {
                let mut report = #crate_path::validation::ValidationReport::empty();

                #(#all_validations)*

                if report.is_empty() {
                    Ok(())
                } else {
                    Err(report)
                }
            }
        }
    }
}
