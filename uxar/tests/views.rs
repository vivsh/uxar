use std::borrow::Cow;
use axum::http::Method;

use uxar::views::{RouterMeta, ViewMeta};

#[test]
fn reverse_url_by_name_and_params() {
    let meta = ViewMeta {
        name: Cow::Owned("test_view".to_string()),
        methods: vec![Method::GET],
        path: Cow::Owned("/users/{id}/profile".to_string()),
        summary: None,
        params: vec![],
        responses: vec![],
    };

    let inspector = RouterMeta::new(vec![meta]);
    let got = inspector.reverse("test_view", &[("id", "42")]);
    assert_eq!(got, Some("/users/42/profile".to_string()));
}
