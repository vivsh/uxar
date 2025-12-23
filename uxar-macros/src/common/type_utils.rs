use syn::Type;

/// Check if a type is Option<T>
pub fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}

/// Extract inner type from Option<T>, returns (is_option, inner_type)
pub fn option_inner_type(ty: &Type) -> (bool, &Type) {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return (true, inner);
                    }
                }
            }
        }
    }
    (false, ty)
}

/// Resolve crate path with fallback
pub fn resolve_crate_path(provided: Option<syn::Path>, default: &str) -> syn::Path {
    provided.unwrap_or_else(|| syn::parse_str(default).expect("valid path"))
}
