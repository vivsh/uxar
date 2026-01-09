use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, Result};

use crate::schemable::ParsedStruct;

/// Derive macro entry point for Validate trait.
pub fn derive_validate_impl(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);

    match ParsedStruct::from_derive_input(input) {
        Ok(parsed) => match generate_validate(&parsed) {
            Ok(expanded) => expanded.into(),
            Err(e) => e.to_compile_error().into(),
        },
        Err(e) => e.to_compile_error().into(),
    }
}

/// Generate the impl block for Validate trait.
fn generate_validate(parsed: &ParsedStruct) -> Result<proc_macro2::TokenStream> {
    let ident = &parsed.ident;
    let (impl_generics, ty_generics, where_clause) = parsed.generics.split_for_impl();

    let field_count = parsed.fields.len();
    let mut field_validations = Vec::with_capacity(field_count);

    for field in &parsed.fields {
        if let Some(tokens) = gen_field_validation(field)? {
            field_validations.push(tokens);
        }
    }

    let expanded = quote! {
        impl #impl_generics ::uxar::validation::Validate for #ident #ty_generics #where_clause {
            fn validate(&self) -> ::std::result::Result<(), ::uxar::validation::ValidationReport> {
                let mut main_report = ::uxar::validation::ValidationReport::empty();

                #(#field_validations)*

                if main_report.is_empty() {
                    Ok(())
                } else {
                    Err(main_report)
                }
            }
        }
    };

    Ok(expanded)
}

/// Generate validation logic for a single field.
fn gen_field_validation(
    field: &crate::schemable::FieldMeta,
) -> Result<Option<proc_macro2::TokenStream>> {
    let field_ident = match &field.ident {
        Some(id) => id,
        None => return Ok(None),
    };
    let field_name = field_ident.to_string();
    let validate = &field.validate;

    let mut checks = Vec::with_capacity(8);

    if validate.delegate {
        checks.push(gen_delegate_check(&field_ident, &field_name));
    }

    if let Some(path) = &validate.custom {
        checks.push(gen_custom_check(&field_ident, &field_name, path));
    }

    if !validate.delegate && validate.custom.is_none() {
        gen_standard_validators(&mut checks, &field_ident, &field_name, validate);
    }

    if checks.is_empty() {
        return Ok(None);
    }

    Ok(Some(quote! {
        {
            #(#checks)*
        }
    }))
}

/// Generate delegation validation check.
fn gen_delegate_check(
    field_ident: &syn::Ident,
    field_name: &str,
) -> proc_macro2::TokenStream {
    quote! {
        if let Err(report) = ::uxar::validation::Validate::validate(&self.#field_ident) {
            main_report.merge(report, Some(::uxar::validation::PathSeg::Field(#field_name.into())));
        }
    }
}

/// Generate custom validation function check.
fn gen_custom_check(
    field_ident: &syn::Ident,
    field_name: &str,
    path: &syn::Path,
) -> proc_macro2::TokenStream {
    quote! {
        if let Err(e) = (#path)(&self.#field_ident) {
            main_report.push(::uxar::validation::Path::root().at_field(#field_name), e);
        }
    }
}

/// Generate all standard validators for a field.
fn gen_standard_validators(
    checks: &mut Vec<proc_macro2::TokenStream>,
    field_ident: &syn::Ident,
    field_name: &str,
    validate: &crate::schemable::ValidateAttrs,
) {
    checks.push(quote! {
        let target = ::uxar::validation::AsValidationTarget::as_validation_target(&self.#field_ident);
    });

    gen_string_validators(checks, field_name, validate);
    gen_string_formats(checks, field_name, validate);
    gen_numeric_validators(checks, field_name, validate);
    gen_collection_validators(checks, field_name, validate);

    if let Some(pattern) = &validate.pattern {
        checks.push(gen_pattern_validator(field_name, pattern));
    }
}

/// Generate string length validators.
fn gen_string_validators(
    checks: &mut Vec<proc_macro2::TokenStream>,
    field_name: &str,
    validate: &crate::schemable::ValidateAttrs,
) {
    if let Some(min_len) = &validate.min_length {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::uxar::validators::min_len(#min_len as usize)(v.as_ref()) {
                    main_report.push(::uxar::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if let Some(max_len) = &validate.max_length {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::uxar::validators::max_len(#max_len as usize)(v.as_ref()) {
                    main_report.push(::uxar::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if let Some(exact_len) = &validate.exact_length {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::uxar::validators::exact_len(#exact_len as usize)(v.as_ref()) {
                    main_report.push(::uxar::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
}

/// Generate string format validators.
fn gen_string_formats(
    checks: &mut Vec<proc_macro2::TokenStream>,
    field_name: &str,
    validate: &crate::schemable::ValidateAttrs,
) {
    if validate.email {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::uxar::validators::email(v.as_ref()) {
                    main_report.push(::uxar::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if validate.url {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::uxar::validators::url(v.as_ref()) {
                    main_report.push(::uxar::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if validate.uuid {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::uxar::validators::uuid(v.as_ref()) {
                    main_report.push(::uxar::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if validate.ipv4 {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::uxar::validators::ipv4(v.as_ref()) {
                    main_report.push(::uxar::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
}

/// Generate numeric range validators.
fn gen_numeric_validators(
    checks: &mut Vec<proc_macro2::TokenStream>,
    field_name: &str,
    validate: &crate::schemable::ValidateAttrs,
) {
    if let Some(min) = &validate.min {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::uxar::validators::min(#min)(v) {
                    main_report.push(::uxar::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if let Some(max) = &validate.max {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::uxar::validators::max(#max)(v) {
                    main_report.push(::uxar::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
}

/// Generate collection size validators.
fn gen_collection_validators(
    checks: &mut Vec<proc_macro2::TokenStream>,
    field_name: &str,
    validate: &crate::schemable::ValidateAttrs,
) {
    if let Some(min_items) = &validate.min_items {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::uxar::validators::min_items(#min_items as usize)(v) {
                    main_report.push(::uxar::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if let Some(max_items) = &validate.max_items {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::uxar::validators::max_items(#max_items as usize)(v) {
                    main_report.push(::uxar::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
}

/// Generate pattern (regex) validator with safe compilation.
fn gen_pattern_validator(field_name: &str, pattern: &syn::LitStr) -> proc_macro2::TokenStream {
    quote! {
        {
            static RE: ::once_cell::sync::OnceCell<::std::result::Result<::regex::Regex, ::regex::Error>> =
                ::once_cell::sync::OnceCell::new();
            let re_result = RE.get_or_init(|| ::regex::Regex::new(#pattern));
            
            if let Some(v) = target {
                match re_result {
                    Ok(re) => {
                        if let Err(e) = ::uxar::validators::regex(re)(v.as_ref()) {
                            main_report.push(::uxar::validation::Path::root().at_field(#field_name), e);
                        }
                    }
                    Err(_) => {
                        main_report.push(
                            ::uxar::validation::Path::root().at_field(#field_name),
                            ::uxar::validation::ValidationError::new("pattern", "Invalid regex pattern")
                        );
                    }
                }
            }
        }
    }
}
