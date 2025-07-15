mod embed;

extern crate proc_macro;


/// Embed a directory at compile time, returning a `Dir` enum. The path should be a literal string 
/// and relative to the crate root.
/// embed!("dir")                 → Dir::new (debug) / Dir::Embed (release)
/// embed!("dir", true)           → Dir::Embed (always)
/// embed!("dir", false)          → Dir::new  (debug) / Dir::Embed (release)
#[proc_macro]
pub fn embed(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    embed::embed(input)
}