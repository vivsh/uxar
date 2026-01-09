use uxar::db::Model;

#[derive(Model)]
struct SimpleTest {
    id: i32,
}

#[test]
fn test_model_macro_exists() {
    // This test just checks that the macro can be invoked
    let _t = SimpleTest { id: 1 };
}
