use darling::FromDeriveInput;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Variant};

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(bitrole), supports(enum_unit))]
struct BitRoleArgs {}

pub fn derive_bitrole(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    
    let args = match BitRoleArgs::from_derive_input(&input) {
        Ok(args) => args,
        Err(err) => return err.write_errors().into(),
    };

    match derive_bitrole_impl(&input, args) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn derive_bitrole_impl(
    input: &DeriveInput,
    _args: BitRoleArgs,
) -> Result<TokenStream, syn::Error> {
    let enum_name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Ensure it's an enum
    let variants = match &input.data {
        Data::Enum(data_enum) => &data_enum.variants,
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "BitRole can only be derived for enums",
            ))
        }
    };

    // Validate all variants are unit variants
    for variant in variants.iter() {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(syn::Error::new_spanned(
                variant,
                "BitRole only supports unit variants (no fields)",
            ));
        }
    }

    // Extract discriminants (bit positions) and validate
    let mut role_value_arms = Vec::new();
    let mut role_pairs_entries = Vec::new();
    let mut mask_arms = Vec::new();
    
    for (idx, variant) in variants.iter().enumerate() {
        let variant_ident = &variant.ident;
        let variant_name = variant_ident.to_string();
        let bit_position = get_discriminant(variant, idx)?;
        
        // Validate bit position is within RoleType range
        if bit_position >= 64 {
            return Err(syn::Error::new_spanned(
                variant,
                format!("Bit position {} must be < 64 for RoleType (u64)", bit_position),
            ));
        }

        let bit_u8 = u8::try_from(bit_position).map_err(|_| {
            syn::Error::new_spanned(variant, "Bit position must fit within u8")
        })?;

        role_value_arms.push(quote! {
            #enum_name::#variant_ident => #bit_u8
        });

        role_pairs_entries.push(quote! {
            (#bit_u8, #variant_name)
        });

        let mask_val = 1u64 << bit_position;
        mask_arms.push(quote! {
            #enum_name::#variant_ident => #mask_val as ::uxar::roles::RoleType
        });
    }

    let expanded = quote! {
        // Auto-derive standard traits
        impl #impl_generics ::core::fmt::Debug for #enum_name #ty_generics #where_clause {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self.role_name() {
                    Some(name) => write!(f, "{}", name),
                    None => write!(f, "Unknown({:?})", self.role_value()),
                }
            }
        }

        impl #impl_generics ::core::fmt::Display for #enum_name #ty_generics #where_clause {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self.role_name() {
                    Some(name) => write!(f, "{}", name),
                    None => write!(f, "Unknown({})", self.role_value()),
                }
            }
        }

        impl #impl_generics ::core::clone::Clone for #enum_name #ty_generics #where_clause {
            fn clone(&self) -> Self {
                *self
            }
        }

        impl #impl_generics ::core::marker::Copy for #enum_name #ty_generics #where_clause {}

        impl #impl_generics #enum_name #ty_generics #where_clause {
            #[doc(hidden)]
            pub const fn __uxar_mask(role: Self) -> ::uxar::roles::RoleType {
                match role {
                    #(#mask_arms,)*
                }
            }
        }

        // BitRole trait implementation
        impl #impl_generics ::uxar::roles::BitRole for #enum_name #ty_generics #where_clause {
            fn role_value(self) -> u8 {
                match self {
                    #(#role_value_arms,)*
                }
            }

            fn role_pairs() -> &'static [(u8, &'static str)] {
                &[#(#role_pairs_entries,)*]
            }
        }
    };

    Ok(expanded)
}

fn get_discriminant(variant: &Variant, default_index: usize) -> Result<usize, syn::Error> {
    if let Some((_, expr)) = &variant.discriminant {
        // Try to evaluate constant expression
        match expr {
            syn::Expr::Lit(lit_expr) => {
                if let syn::Lit::Int(lit_int) = &lit_expr.lit {
                    return lit_int.base10_parse::<usize>().map_err(|_| {
                        syn::Error::new_spanned(
                            lit_int,
                            "Invalid discriminant value",
                        )
                    });
                }
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    expr,
                    "Only integer literal discriminants are supported",
                ))
            }
        }
    }
    
    // If no explicit discriminant, use index (0-based bit position)
    Ok(default_index)
}
