//! PgNotify macro implementation using bundlepart infrastructure.
//!
//! Provides #[pgnotify] attribute macro for PostgreSQL NOTIFY handlers.
//! Delegates to bundlepart.rs for consistent code generation.

use darling::FromMeta;
use proc_macro::TokenStream;
use quote::quote;

use crate::bundlepart::{self, FnSpec};

/// PgNotify configuration metadata.
///
/// Maps to uxar::emitters::PgNotifyConf runtime structure.
#[derive(Debug, FromMeta, Default)]
struct PgNotifyConfMeta {
    /// PostgreSQL LISTEN channel name
    #[darling(default)]
    channel: Option<String>,
    
    /// Optional target configuration
    #[darling(default)]
    target: Option<String>,
}

/// Entry point for #[pgnotify] macro.
///
/// Handles both free functions and methods in impl blocks.
pub(crate) fn parse_pgnotify(attr: TokenStream, item: TokenStream) -> TokenStream {
    bundlepart::generate_bundle_part::<PgNotifyConfMeta>(
        attr,
        item,
        "pgnotify",
        build_pgnotify_conf,
    )
}

/// Build PgNotifyConf from parsed metadata and function spec.
///
/// Validates channel name and generates PgNotifyConf construction.
fn build_pgnotify_conf(
    conf: &PgNotifyConfMeta,
    _spec: &FnSpec,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let channel = conf.channel.as_ref().ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "PostgreSQL LISTEN channel is required. Use: #[pgnotify(channel = \"my_channel\")]"
        )
    })?;
    
    validate_channel_name(channel)?;
    
    let target = if let Some(target_str) = &conf.target {
        quote! { ::uxar::emitters::EmitTarget::from_str(#target_str).unwrap_or_default() }
    } else {
        quote! { ::uxar::emitters::EmitTarget::default() }
    };

    Ok(quote! {
        ::uxar::emitters::PgNotifyConf {
            channel: #channel.to_string(),
            target: #target,
        }
    })
}

/// Validate PostgreSQL channel name.
///
/// Ensures channel name is not empty and follows basic constraints.
fn validate_channel_name(channel: &str) -> Result<(), syn::Error> {
    if channel.trim().is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "PostgreSQL channel name cannot be empty"
        ));
    }
    
    Ok(())
}
