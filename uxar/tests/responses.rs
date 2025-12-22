use std::borrow::Cow;
use std::collections::BTreeMap;

use axum::http::Method;

use uxar::views::{ReturnMeta, ViewMeta, routable, route};

// Test: when handler returns Json<i32> and no explicit responses, default response
// (status 200) should be derived from the function return type (Json<i32>)
#[test]
fn default_response_from_return_type() {
    #[allow(unused_imports)]
    use uxar::views::Json;

    struct S;

    // Apply macro to generate metadata
    #[routable]
    impl S {
        #[route]
        async fn foo() -> Json<i32> { Json(42) }
    }

    let (_router, metas) = <S as uxar::views::StaticRoutable>::as_routable();
    let meta = metas.into_iter().find(|m| m.name == Cow::<'static, str>::Owned("foo".to_string())).expect("meta foo");

    // Expect a default (no-status) response entry and a schema (i32 -> Some)
    let resp_opt = meta.responses.into_iter().find(|r: &ReturnMeta| r.status.is_none());
    assert!(resp_opt.is_some(), "expected default (no-status) response");
    let resp = resp_opt.unwrap();
    assert!(resp.schema.is_some(), "expected schema for Json<i32> -> i32");
}

// Test: a default (no-status) response override supplies the type for statused
// responses that omit `ty`.
#[test]
fn statused_inherits_default_type() {
    #[allow(unused_imports)]
    use uxar::views::Json;

    struct S2;

    #[routable]
    impl S2 {
        // default (no-status) says i64; status 201 omits ty and should inherit i64
        #[route(response(ty = "i64"), response(status = 201))]
        async fn bar() -> Json<i32> { Json(7) }
    }

    let (_router, metas) = <S2 as uxar::views::StaticRoutable>::as_routable();
    let meta = metas.into_iter().find(|m| m.name == Cow::<'static, str>::Owned("bar".to_string())).expect("meta bar");

    let r201 = meta.responses.into_iter().find(|r: &ReturnMeta| r.status == Some(201)).expect("201 response");
    // The type_name will stringify to include "i64"
    assert!(r201.type_name.to_string().contains("i64"), "expected inherited i64 type for 201");
    assert!(r201.schema.is_some(), "expected schema for inherited i64");
}
