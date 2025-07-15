use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{parse::Parse, parse_macro_input, Expr, ExprLit, Lit, LitBool, LitStr, Token};


pub fn embed(input: TokenStream) -> TokenStream { 
    let args = parse_macro_input!(input as EmbedArgs);

    let rel_lit: LitStr = match args.path {
        Lit::Str(s) => s,
        other       => return compile_error("first argument must be a string literal", other.span()),
    };
    
    let rel_path = rel_lit.value();
    let call_span = rel_lit.span();          // proc_macro2::Span

    // ── validate directory exists inside crate root ────────────────────────
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("uxar_macros::embed!: CARGO_MANIFEST_DIR not set");
    let full_path    = std::path::Path::new(&manifest_dir).join(&rel_path).canonicalize().expect(&format!("uxar_macros::embed!: failed to canonicalize path: {rel_path}, manifest_dir: {manifest_dir}"));

    let full_path = full_path.to_str().expect("uxar_macros::embed!: failed to convert path to string");

    if !full_path.starts_with(&manifest_dir) {
        let msg = format!(
            "uxar_macros::embed!: directory not found:\n  {full_path}\n  expected to be inside crate root:\n  {manifest_dir}\n  relative path: {rel_path}",
        );
        return compile_error(&msg, call_span);
    };

    let full_literal: LitStr = LitStr::new(
       full_path,
        call_span,
    );

    let embed_code = quote! {
        ::uxar::embed::Dir::Embed(include_dir::include_dir!(#full_literal))
    };

    let debug_code = quote! {
        ::uxar::embed::Dir::new(#full_literal)
    };

    match args.force {
        Some(true)  => embed_code.into(),
        Some(false) | None => {
            quote! {
                if cfg!(debug_assertions) {
                    #debug_code
                } else {
                    #embed_code
                }
            }
            .into()
        }
    }
}

/// Emit `compile_error!($msg)` at the given span.
fn compile_error<S: AsRef<str>>(msg: S, span: Span) -> TokenStream {
    let lit = LitStr::new(msg.as_ref(), span);
    quote!( compile_error!(#lit) ).into()
}

/*──────────────────────── argument parser ───────────────────────────────*/

struct EmbedArgs {
    path: Lit,
    force: Option<bool>,
}

impl Parse for EmbedArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let path: Lit = input.parse()?;

        // optional comma + bool
        let force = if input.peek(Token![,]) {
            let _: Token![,] = input.parse()?;
            let flag: Expr = input.parse()?;
            match flag {
                Expr::Lit(ExprLit { lit: Lit::Bool(LitBool { value, .. }), .. }) => Some(value),
                _ => return Err(syn::Error::new_spanned(flag, "second argument must be `true` or `false`")),
            }
        } else {
            None
        };

        Ok(EmbedArgs { path, force })
    }
}