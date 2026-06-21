use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, Lit, Result};

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
    let mut schema_validations = Vec::with_capacity(field_count);

    for field in &parsed.fields {
        if let Some(tokens) = gen_field_validation(field)? {
            field_validations.push(tokens);
        }
        if let Some(tokens) = gen_field_schema_validation(field) {
            schema_validations.push(tokens);
        }
    }

    let expanded = quote! {
        impl #impl_generics ::vyuh::validation::Validate for #ident #ty_generics #where_clause {
            fn validate(&self) -> ::std::result::Result<(), ::vyuh::validation::ValidationReport> {
                let mut main_report = ::vyuh::validation::ValidationReport::empty();

                #(#field_validations)*

                if main_report.is_empty() {
                    Ok(())
                } else {
                    Err(main_report)
                }
            }
        }

        impl #impl_generics ::vyuh::validation::ValidationSchema for #ident #ty_generics #where_clause {
            fn apply_validation_schema(
                schema: &mut ::serde_json::Value,
                definitions: &mut ::serde_json::Map<String, ::serde_json::Value>,
            ) {
                #(#schema_validations)*
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
        checks.push(gen_delegate_check(field_ident, &field_name));
    }

    if let Some(path) = &validate.custom {
        checks.push(gen_custom_check(field_ident, &field_name, path));
    }

    if !validate.delegate && has_standard_validators(validate) {
        gen_standard_validators(&mut checks, field_ident, &field_name, validate);
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
fn gen_delegate_check(field_ident: &syn::Ident, field_name: &str) -> proc_macro2::TokenStream {
    quote! {
        if let Err(report) = ::vyuh::validation::Validate::validate(&self.#field_ident) {
            main_report.merge(report, Some(::vyuh::validation::PathSeg::Field(#field_name.into())));
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
            main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
        }
    }
}

/// Check if validate attrs has any standard validators.
fn has_standard_validators(validate: &crate::schemable::ValidateAttrs) -> bool {
    validate.min_length.is_some()
        || validate.max_length.is_some()
        || validate.exact_length.is_some()
        || validate.pattern.is_some()
        || validate.email
        || validate.url
        || validate.uuid
        || validate.phone_e164
        || validate.ipv4
        || validate.ipv6
        || validate.date
        || validate.datetime
        || validate.min.is_some()
        || validate.max.is_some()
        || validate.multiple_of.is_some()
        || validate.min_items.is_some()
        || validate.max_items.is_some()
        || validate.unique_items
        || !validate.enumeration.0.is_empty()
}

/// Generate all standard validators for a field.
fn gen_standard_validators(
    checks: &mut Vec<proc_macro2::TokenStream>,
    field_ident: &syn::Ident,
    field_name: &str,
    validate: &crate::schemable::ValidateAttrs,
) {
    checks.push(quote! {
        let target = ::vyuh::validation::AsValidationTarget::as_validation_target(&self.#field_ident);
    });

    gen_string_validators(checks, field_name, validate);
    gen_string_formats(checks, field_name, validate);
    gen_numeric_validators(checks, field_name, validate);
    gen_collection_validators(checks, field_name, validate);
    gen_enum_validator(checks, field_name, validate);

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
                if let Err(e) = ::vyuh::validators::min_len(#min_len as usize)(v.as_ref()) {
                    main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if let Some(max_len) = &validate.max_length {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::vyuh::validators::max_len(#max_len as usize)(v.as_ref()) {
                    main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if let Some(exact_len) = &validate.exact_length {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::vyuh::validators::exact_len(#exact_len as usize)(v.as_ref()) {
                    main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
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
                if let Err(e) = ::vyuh::validators::email(v.as_ref()) {
                    main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if validate.url {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::vyuh::validators::url(v.as_ref()) {
                    main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if validate.uuid {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::vyuh::validators::uuid(v.as_ref()) {
                    main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if validate.ipv4 {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::vyuh::validators::ipv4(v.as_ref()) {
                    main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if validate.ipv6 {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::vyuh::validators::ipv6(v.as_ref()) {
                    main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if validate.phone_e164 {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::vyuh::validators::phone_e164(v.as_ref()) {
                    main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if validate.date {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::vyuh::validators::date(v.as_ref()) {
                    main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if validate.datetime {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::vyuh::validators::datetime(v.as_ref()) {
                    main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
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
        if validate.exclusive_min {
            checks.push(quote! {
                if let Some(v) = target {
                    if let Err(e) = ::vyuh::validators::min_exclusive(#min)(v) {
                        main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
                    }
                }
            });
        } else {
            checks.push(quote! {
                if let Some(v) = target {
                    if let Err(e) = ::vyuh::validators::min(#min)(v) {
                        main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
                    }
                }
            });
        }
    }
    if let Some(max) = &validate.max {
        if validate.exclusive_max {
            checks.push(quote! {
                if let Some(v) = target {
                    if let Err(e) = ::vyuh::validators::max_exclusive(#max)(v) {
                        main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
                    }
                }
            });
        } else {
            checks.push(quote! {
                if let Some(v) = target {
                    if let Err(e) = ::vyuh::validators::max(#max)(v) {
                        main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
                    }
                }
            });
        }
    }
    if let Some(multiple_of) = &validate.multiple_of {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::vyuh::validators::multiple_of(#multiple_of as _)(v) {
                    main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
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
                if let Err(e) = ::vyuh::validators::min_items(#min_items as usize)(v) {
                    main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if let Some(max_items) = &validate.max_items {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::vyuh::validators::max_items(#max_items as usize)(v) {
                    main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
    if validate.unique_items {
        checks.push(quote! {
            if let Some(v) = target {
                if let Err(e) = ::vyuh::validators::unique_items(v) {
                    main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
                }
            }
        });
    }
}

fn gen_enum_validator(
    checks: &mut Vec<proc_macro2::TokenStream>,
    field_name: &str,
    validate: &crate::schemable::ValidateAttrs,
) {
    if validate.enumeration.0.is_empty() {
        return;
    }

    let comparisons = validate.enumeration.0.iter().map(|lit| match lit {
        Lit::Str(value) => {
            quote! { {
                let value: &str = v.as_ref();
                value == #value
            } }
        }
        Lit::Int(value) => {
            quote! { *v == #value }
        }
        Lit::Float(value) => {
            quote! { *v == #value }
        }
        Lit::Bool(value) => {
            quote! { *v == #value }
        }
        _ => {
            quote! { false }
        }
    });

    let choices = validate.enumeration.0.len();
    checks.push(quote! {
        if let Some(v) = target {
            if !(#(#comparisons)||*) {
                main_report.push(
                    ::vyuh::validation::Path::root().at_field(#field_name),
                    ::vyuh::validation::ValidationError::new(
                        "invalid_choice",
                        "Selected value is not a valid choice.",
                    ).with_param("choices", #choices)
                );
            }
        }
    });
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
                        if let Err(e) = ::vyuh::validators::regex(re)(v.as_ref()) {
                            main_report.push(::vyuh::validation::Path::root().at_field(#field_name), e);
                        }
                    }
                    Err(_) => {
                        main_report.push(
                            ::vyuh::validation::Path::root().at_field(#field_name),
                            ::vyuh::validation::ValidationError::new("pattern", "Invalid regex pattern")
                        );
                    }
                }
            }
        }
    }
}

fn gen_field_schema_validation(
    field: &crate::schemable::FieldMeta,
) -> Option<proc_macro2::TokenStream> {
    let field_ident = field.ident.as_ref()?;
    let field_name = field_ident.to_string();
    let field_ty = &field.ty;
    let validate = &field.validate;

    let mut constraints = Vec::new();

    if let Some(min_len) = &validate.min_length {
        constraints.push(quote! { ("minLength", ::serde_json::json!(#min_len)) });
    }
    if let Some(max_len) = &validate.max_length {
        constraints.push(quote! { ("maxLength", ::serde_json::json!(#max_len)) });
    }
    if let Some(exact_len) = &validate.exact_length {
        constraints.push(quote! { ("minLength", ::serde_json::json!(#exact_len)) });
        constraints.push(quote! { ("maxLength", ::serde_json::json!(#exact_len)) });
    }
    if let Some(pattern) = &validate.pattern {
        constraints.push(quote! { ("pattern", ::serde_json::json!(#pattern)) });
    }

    if validate.email {
        constraints.push(quote! { ("format", ::serde_json::json!("email")) });
    } else if validate.url {
        constraints.push(quote! { ("format", ::serde_json::json!("uri")) });
    } else if validate.uuid {
        constraints.push(quote! { ("format", ::serde_json::json!("uuid")) });
    } else if validate.phone_e164 {
        constraints.push(quote! { ("pattern", ::serde_json::json!(r"^\+[1-9]\d{1,14}$")) });
    } else if validate.ipv4 {
        constraints.push(quote! { ("format", ::serde_json::json!("ipv4")) });
    } else if validate.ipv6 {
        constraints.push(quote! { ("format", ::serde_json::json!("ipv6")) });
    } else if validate.date {
        constraints.push(quote! { ("format", ::serde_json::json!("date")) });
    } else if validate.datetime {
        constraints.push(quote! { ("format", ::serde_json::json!("date-time")) });
    }

    if let Some(min) = &validate.min {
        constraints.push(quote! { ("minimum", ::serde_json::json!(#min)) });
        if validate.exclusive_min {
            constraints.push(quote! { ("exclusiveMinimum", ::serde_json::json!(true)) });
        }
    }
    if let Some(max) = &validate.max {
        constraints.push(quote! { ("maximum", ::serde_json::json!(#max)) });
        if validate.exclusive_max {
            constraints.push(quote! { ("exclusiveMaximum", ::serde_json::json!(true)) });
        }
    }
    if let Some(multiple_of) = &validate.multiple_of {
        constraints.push(quote! { ("multipleOf", ::serde_json::json!(#multiple_of)) });
    }
    if let Some(min_items) = &validate.min_items {
        constraints.push(quote! { ("minItems", ::serde_json::json!(#min_items)) });
    }
    if let Some(max_items) = &validate.max_items {
        constraints.push(quote! { ("maxItems", ::serde_json::json!(#max_items)) });
    }
    if validate.unique_items {
        constraints.push(quote! { ("uniqueItems", ::serde_json::json!(true)) });
    }
    if !validate.enumeration.0.is_empty() {
        let values = validate.enumeration.0.iter();
        constraints.push(quote! { ("enum", ::serde_json::json!([#(#values),*])) });
    }

    let constraints_tokens = (!constraints.is_empty()).then(|| {
        quote! {
            ::vyuh::validation::apply_field_constraints(
                schema,
                definitions,
                #field_name,
                &[#(#constraints),*],
            );
        }
    });

    let delegate_tokens = validate.delegate.then(|| {
        quote! {
            ::vyuh::validation::apply_field_validation_schema::<#field_ty>(
                schema,
                definitions,
                #field_name,
            );
        }
    });

    let custom_schema_tokens = validate.custom_schema.as_ref().map(|name| {
        quote! {
            ::vyuh::validation::apply_field_custom_validator(
                schema,
                definitions,
                #field_name,
                #name,
            );
        }
    });

    if constraints_tokens.is_none() && delegate_tokens.is_none() && custom_schema_tokens.is_none() {
        return None;
    }

    Some(quote! {
        #constraints_tokens
        #delegate_tokens
        #custom_schema_tokens
    })
}
