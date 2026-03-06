//! Signal macro implementation using bundlepart infrastructure.
//!
//! Provides #[signal] attribute macro for signal handlers.
//! Delegates to bundlepart.rs for consistent code generation.

use darling::FromMeta;
use proc_macro::TokenStream;
use quote::quote;

use crate::bundlepart::{self, FnSpec};

/// Signal configuration metadata.
///
/// Maps to uxar::signals::SignalConf runtime structure.
#[derive(Debug, FromMeta, Default)]
struct SignalConfMeta {
    // SignalConf is currently empty, but keeping this for future extensibility
}

/// Entry point for #[signal] macro.
///
/// Handles both free functions and methods in impl blocks.
pub(crate) fn parse_signal(attr: TokenStream, item: TokenStream) -> TokenStream {
    bundlepart::generate_bundle_part::<SignalConfMeta>(
        attr,
        item,
        "signal",
        build_signal_conf,
    )
}

/// Build SignalConf from parsed metadata and function spec.
///
/// SignalConf is currently empty but can be extended in the future.
fn build_signal_conf(
    _conf: &SignalConfMeta,
    _spec: &FnSpec,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    Ok(quote! {
        ::uxar::signals::SignalConf::default()
    })
}
