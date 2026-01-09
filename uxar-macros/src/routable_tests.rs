use super::*;
use syn::parse_quote;

#[test]
fn test_validate_http_method_valid() {
    let span = proc_macro2::Span::call_site();
    assert!(validate_http_method("GET", span).is_ok());
    assert!(validate_http_method("get", span).is_ok());
    assert!(validate_http_method("POST", span).is_ok());
    assert!(validate_http_method("PUT", span).is_ok());
    assert!(validate_http_method("DELETE", span).is_ok());
    assert!(validate_http_method("PATCH", span).is_ok());
    assert!(validate_http_method("HEAD", span).is_ok());
    assert!(validate_http_method("OPTIONS", span).is_ok());
}

#[test]
fn test_validate_http_method_invalid() {
    let span = proc_macro2::Span::call_site();
    let result = validate_http_method("INVALID", span);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Unsupported HTTP method"));
    assert!(err.to_string().contains("INVALID"));
}

#[test]
fn test_validate_path_valid() {
    let span = proc_macro2::Span::call_site();
    assert!(validate_path("/", span).is_ok());
    assert!(validate_path("/users", span).is_ok());
    assert!(validate_path("/users/:id", span).is_ok());
    assert!(validate_path("/api/v1/users", span).is_ok());
    assert!(validate_path("/users/:id/posts/:post_id", span).is_ok());
}

#[test]
fn test_validate_path_empty() {
    let span = proc_macro2::Span::call_site();
    let result = validate_path("", span);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("cannot be empty"));
}

#[test]
fn test_validate_path_no_leading_slash() {
    let span = proc_macro2::Span::call_site();
    let result = validate_path("users", span);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("must start with '/'"));
}

#[test]
fn test_validate_path_double_slash() {
    let span = proc_macro2::Span::call_site();
    let result = validate_path("/users//posts", span);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("double slashes"));
}

#[test]
fn test_normalize_methods_empty() {
    let methods = vec![];
    let result = normalize_methods(&methods);
    assert_eq!(result, vec!["GET"]);
}

#[test]
fn test_normalize_methods_lowercase() {
    let methods = vec!["get".to_string(), "post".to_string()];
    let result = normalize_methods(&methods);
    assert_eq!(result, vec!["GET", "POST"]);
}

#[test]
fn test_normalize_methods_mixed_case() {
    let methods = vec!["Get".to_string(), "pOsT".to_string()];
    let result = normalize_methods(&methods);
    assert_eq!(result, vec!["GET", "POST"]);
}

#[test]
fn test_build_full_path_default() {
    let result = build_full_path("my_handler", None, None);
    assert_eq!(result, "/my_handler");
}

#[test]
fn test_build_full_path_custom_path() {
    let result = build_full_path("my_handler", Some("/custom"), None);
    assert_eq!(result, "/custom");
}

#[test]
fn test_build_full_path_base_path() {
    let result = build_full_path("my_handler", None, Some("/api"));
    assert_eq!(result, "/api/my_handler");
}

#[test]
fn test_build_full_path_both() {
    let result = build_full_path("my_handler", Some("/custom"), Some("/api"));
    assert_eq!(result, "/api/custom");
}

#[test]
fn test_build_full_path_base_path_no_slash() {
    let result = build_full_path("my_handler", None, Some("api"));
    assert_eq!(result, "api/my_handler");
}

#[test]
fn test_signature_no_body_query() {
    let sig: syn::Signature = parse_quote! {
        fn handler(Query(params): Query<SearchParams>) -> impl IntoResponse
    };
    assert!(!signature_accepts_body(&sig));
}

#[test]
fn test_signature_no_body_state() {
    let sig: syn::Signature = parse_quote! {
        fn handler(State(db): State<Database>) -> impl IntoResponse
    };
    assert!(!signature_accepts_body(&sig));
}



#[test]
fn test_signature_accepts_body_with_extractors() {
    let sig: syn::Signature = parse_quote! {
        fn handler(_site: Site, Json(payload): Json<CreateUser>) -> impl IntoResponse
    };
    assert!(signature_accepts_body(&sig));
}

#[test]
fn test_signature_no_body_with_path_query() {
    let sig: syn::Signature = parse_quote! {
        fn handler(_site: Site, Path(id): Path<i32>, Query(params): Query<Filters>) -> impl IntoResponse
    };
    assert!(!signature_accepts_body(&sig));
}

#[test]
fn test_doc_from_fn_no_comments() {
    let fn_item: syn::ImplItemFn = parse_quote! {
        fn handler() {}
    };
    let (summary, description) = doc_from_fn(&fn_item);
    assert_eq!(summary, None);
    assert_eq!(description, None);
}

#[test]
fn test_doc_from_fn_single_line() {
    let fn_item: syn::ImplItemFn = parse_quote! {
        #[doc = "Get user by ID"]
        fn handler() {}
    };
    let (summary, description) = doc_from_fn(&fn_item);
    assert_eq!(summary, Some("Get user by ID".to_string()));
    assert_eq!(description, None);
}

#[test]
fn test_doc_from_fn_multiple_lines() {
    let fn_item: syn::ImplItemFn = parse_quote! {
        #[doc = "Get user by ID"]
        #[doc = ""]
        #[doc = "Returns the user with the specified ID"]
        fn handler() {}
    };
    let (summary, description) = doc_from_fn(&fn_item);
    assert_eq!(summary, Some("Get user by ID".to_string()));
    assert_eq!(description, Some("Returns the user with the specified ID".to_string()));
}

#[test]
fn test_doc_from_fn_with_whitespace() {
    let fn_item: syn::ImplItemFn = parse_quote! {
        #[doc = "  Get user by ID  "]
        fn handler() {}
    };
    let (summary, description) = doc_from_fn(&fn_item);
    assert_eq!(summary, Some("Get user by ID".to_string()));
    assert_eq!(description, None);
}

#[test]
fn test_doc_from_fn_mixed_with_other_attrs() {
    let fn_item: syn::ImplItemFn = parse_quote! {
        #[doc = "List all users"]
        #[allow(unused)]
        #[doc = "with optional filtering"]
        fn handler() {}
    };
    let (summary, description) = doc_from_fn(&fn_item);
    // Both lines are part of same paragraph (no double newline), so first line -> summary
    assert_eq!(summary, Some("List all users".to_string()));
    // Second line becomes description
    assert_eq!(description, Some("with optional filtering".to_string()));
}

#[test]
fn test_doc_from_fn_empty_lines_filtered() {
    let fn_item: syn::ImplItemFn = parse_quote! {
        #[doc = "First line"]
        #[doc = "   "]
        #[doc = ""]
        #[doc = "Last line"]
        fn handler() {}
    };
    let (summary, description) = doc_from_fn(&fn_item);
    // Double newline splits paragraphs, so "First line" -> summary, "Last line" -> description
    assert_eq!(summary, Some("First line".to_string()));
    assert_eq!(description, Some("Last line".to_string()));
}

#[test]
fn test_doc_from_fn_splits_on_double_newline() {
    // Doc comments split on double newline: first paragraph -> summary, rest -> description
    let fn_item: syn::ImplItemFn = parse_quote! {
        #[doc = "Get user by ID"]
        #[doc = ""]
        #[doc = "Retrieves a user from the database."]
        #[doc = "Requires authentication."]
        fn handler() {}
    };
    let (summary, description) = doc_from_fn(&fn_item);
    assert_eq!(summary, Some("Get user by ID".to_string()));
    assert_eq!(description, Some("Retrieves a user from the database.\nRequires authentication.".to_string()));
}

#[test]
fn test_doc_from_fn_single_paragraph_summary_only() {
    // Single paragraph with no double newline should only set summary
    let fn_item: syn::ImplItemFn = parse_quote! {
        #[doc = "Delete user"]
        fn handler() {}
    };
    let (summary, description) = doc_from_fn(&fn_item);
    assert_eq!(summary, Some("Delete user".to_string()));
    assert_eq!(description, None);
}

#[test]
fn test_doc_from_fn_multiline_first_paragraph() {
    // Multiple lines before double newline should all go into summary
    let fn_item: syn::ImplItemFn = parse_quote! {
        #[doc = "First line"]
        #[doc = "Second line"]
        #[doc = ""]
        #[doc = "Description paragraph"]
        fn handler() {}
    };
    let (summary, description) = doc_from_fn(&fn_item);
    // Only first line of first paragraph becomes summary
    assert_eq!(summary, Some("First line".to_string()));
    // Remaining lines of first paragraph + description go into description
    assert_eq!(description, Some("Second line\n\nDescription paragraph".to_string()));
}

#[test]
fn test_build_summary_tokens_with_explicit_summary() {
    let fn_item: syn::ImplItemFn = parse_quote! {
        #[doc = "This doc comment should be ignored"]
        fn handler() {}
    };
    let summary = Some("Explicit summary".to_string());
    let tokens = build_summary_tokens(summary.as_ref(), &fn_item);
    let code = tokens.to_string();
    assert!(code.contains("Explicit summary"));
    assert!(!code.contains("This doc comment"));
}

#[test]
fn test_build_summary_tokens_from_doc_comment() {
    let fn_item: syn::ImplItemFn = parse_quote! {
        #[doc = "Get user by ID"]
        fn handler() {}
    };
    let tokens = build_summary_tokens(None, &fn_item);
    let code = tokens.to_string();
    assert!(code.contains("Get user by ID"));
}

#[test]
fn test_build_summary_tokens_no_summary_or_doc() {
    let fn_item: syn::ImplItemFn = parse_quote! {
        fn handler() {}
    };
    let tokens = build_summary_tokens(None, &fn_item);
    let code = tokens.to_string();
    assert!(code.contains("None"));
}

#[test]
fn test_normalize_methods_uppercase() {
    let methods = vec!["GET".to_string(), "POST".to_string(), "PUT".to_string()];
    let normalized = normalize_methods(&methods);
    assert_eq!(normalized, vec!["GET", "POST", "PUT"]);
}

#[test]
fn test_build_full_path_with_colon_param() {
    let result = build_full_path("get_user_by_id", Some("/:id"), Some("/users"));
    assert_eq!(result, "/users/:id");
}

#[test]
fn test_build_full_path_multiple_segments() {
    let result = build_full_path("list_posts", Some("/posts"), Some("/api/v1"));
    assert_eq!(result, "/api/v1/posts");
}

#[test]
fn test_validate_path_with_params() {
    let span = proc_macro2::Span::call_site();
    assert!(validate_path("/users/:id", span).is_ok());
    assert!(validate_path("/posts/:post_id/comments/:comment_id", span).is_ok());
}

#[test]
fn test_signature_with_multiple_body_types() {
    let sig: syn::Signature = parse_quote! {
        fn handler(Json(data): Json<CreateUser>, State(db): State<Database>) -> impl IntoResponse
    };
    assert!(signature_accepts_body(&sig));
}

#[test]
fn test_signature_form_extractor() {
    let sig: syn::Signature = parse_quote! {
        fn handler(Form(data): Form<LoginForm>) -> impl IntoResponse
    };
    assert!(signature_accepts_body(&sig));
}

#[test]
fn test_signature_multipart_extractor() {
    let sig: syn::Signature = parse_quote! {
        fn handler(Multipart(data): Multipart) -> impl IntoResponse
    };
    assert!(signature_accepts_body(&sig));
}

#[test]
fn test_signature_bytes_extractor() {
    let sig: syn::Signature = parse_quote! {
        fn handler(Bytes(data): Bytes) -> impl IntoResponse
    };
    assert!(signature_accepts_body(&sig));
}

#[test]
fn test_signature_string_extractor() {
    let sig: syn::Signature = parse_quote! {
        fn handler(body: String) -> impl IntoResponse
    };
    assert!(signature_accepts_body(&sig));
}

#[test]
fn test_signature_vec_u8_extractor() {
    let sig: syn::Signature = parse_quote! {
        fn handler(body: Vec<u8>) -> impl IntoResponse
    };
    // Vec<u8> is not treated as a body extractor
    assert!(!signature_accepts_body(&sig));
}

#[test]
fn test_signature_no_body_only_path() {
    let sig: syn::Signature = parse_quote! {
        fn handler(Path(id): Path<i32>) -> impl IntoResponse
    };
    assert!(!signature_accepts_body(&sig));
}

#[test]
fn test_signature_no_body_extension() {
    let sig: syn::Signature = parse_quote! {
        fn handler(Extension(user): Extension<User>) -> impl IntoResponse
    };
    assert!(!signature_accepts_body(&sig));
}

#[test]
fn test_signature_mixed_extractors_with_body() {
    let sig: syn::Signature = parse_quote! {
        fn handler(
            Path(id): Path<i32>,
            Query(params): Query<Filters>,
            State(db): State<Database>,
            Json(payload): Json<UpdateRequest>
        ) -> Result<Json<Response>, ApiError>
    };
    assert!(signature_accepts_body(&sig));
}

#[test]
fn test_validate_http_method_case_insensitive() {
    let span = proc_macro2::Span::call_site();
    assert!(validate_http_method("Get", span).is_ok());
    assert!(validate_http_method("pOsT", span).is_ok());
    assert!(validate_http_method("DeLeTe", span).is_ok());
}

#[test]
fn test_validate_path_root() {
    let span = proc_macro2::Span::call_site();
    assert!(validate_path("/", span).is_ok());
}

#[test]
fn test_build_full_path_empty_base() {
    let result = build_full_path("handler", Some("/test"), None);
    assert_eq!(result, "/test");
}

#[test]
fn test_build_full_path_root_custom_path() {
    let result = build_full_path("handler", Some("/"), Some("/api"));
    // When path is "/" it gets trimmed to "", so result is "/api/"
    assert_eq!(result, "/api/");
}

#[test]
fn test_normalize_methods_preserves_order() {
    let methods = vec!["DELETE".to_string(), "GET".to_string(), "POST".to_string()];
    let normalized = normalize_methods(&methods);
    assert_eq!(normalized, vec!["DELETE", "GET", "POST"]);
}


