use super::*;
use syn::parse_quote;

#[test]
fn test_container_attrs() {
    let input: DeriveInput = parse_quote! {
        #[schema(table = "my_table", tags("t1", "t2"))]
        struct Test {
            field: i32
        }
    };
    let parsed = ParsedStruct::from_derive_input(input).unwrap();
    assert_eq!(parsed.container.table.unwrap().value(), "my_table");
    assert_eq!(parsed.container.tags.len(), 2);
    assert_eq!(parsed.container.tags[0].value(), "t1");
    assert_eq!(parsed.container.tags[1].value(), "t2");
}

#[test]
fn test_container_attrs_array() {
    let input: DeriveInput = parse_quote! {
        #[schema(table = "my_table", tags = ["t1", "t2"])]
        struct Test {
            field: i32
        }
    };
    let parsed = ParsedStruct::from_derive_input(input).unwrap();
    assert_eq!(parsed.container.table.unwrap().value(), "my_table");
    assert_eq!(parsed.container.tags.len(), 2);
    assert_eq!(parsed.container.tags[0].value(), "t1");
    assert_eq!(parsed.container.tags[1].value(), "t2");
}

#[test]
fn test_validate_attrs_valid() {
    let input: DeriveInput = parse_quote! {
        struct Test {
            #[validate(min = 5, max = 10, email)]
            field: String,
        }
    };
    let parsed = ParsedStruct::from_derive_input(input).unwrap();
    let field = &parsed.fields[0];
    assert_eq!(field.validate.min.as_ref().unwrap().base10_parse::<i32>().unwrap(), 5);
    assert_eq!(field.validate.max.as_ref().unwrap().base10_parse::<i32>().unwrap(), 10);
    assert!(field.validate.email);
}

#[test]
fn test_validate_enum_values_list() {
    let input: DeriveInput = parse_quote! {
        struct Test {
            #[validate(enum_values(1, 2, 3))]
            field: i32,
        }
    };
    let parsed = ParsedStruct::from_derive_input(input).unwrap();
    let field = &parsed.fields[0];
    assert_eq!(field.validate.enumeration.0.len(), 3);
}

#[test]
fn test_validate_enum_values_array() {
    let input: DeriveInput = parse_quote! {
        struct Test {
            #[validate(enum_values = [1, 2, 3])]
            field: i32,
        }
    };
    let parsed = ParsedStruct::from_derive_input(input).unwrap_err().to_string();
    assert!(parsed.contains("Error decoding"), "Msg was: {}", parsed);
}

#[test]
fn test_validate_conflicting_formats() {
    let input: DeriveInput = parse_quote! {
        struct Test {
            #[validate(email, url)]
            field: String,
        }
    };
    let result = ParsedStruct::from_derive_input(input);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "Cannot mix format validators: found email, url");
}

#[test]
fn test_validate_exclusive_min_without_min() {
    let input: DeriveInput = parse_quote! {
        struct Test {
            #[validate(exclusive_min)]
            field: i32,
        }
    };
    let result = ParsedStruct::from_derive_input(input);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "exclusive_min requires min to be set");
}

#[test]
fn test_field_attrs_conflict() {
    let input: DeriveInput = parse_quote! {
        struct Test {
            #[field(skip, flatten)]
            field: String,
        }
    };
    let result = ParsedStruct::from_derive_input(input);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "Cannot use both skip and flatten on the same field");
}

#[test]
fn test_column_attrs_valid() {
    let input: DeriveInput = parse_quote! {
        struct Test {
            #[column(name = "col_name", primary_key, unique_group = "g1")]
            field: String,
        }
    };
    let parsed = ParsedStruct::from_derive_input(input).unwrap();
    let field = &parsed.fields[0];
    assert_eq!(field.column.name.as_ref().unwrap().value(), "col_name");
    assert!(field.column.primary_key);
    assert_eq!(field.column.unique_groups.len(), 1);
    assert_eq!(field.column.unique_groups[0].value(), "g1");
}

#[test]
fn test_cross_attribute_pollution_validate_in_column() {
    // Test that putting a validate attribute (min) in column fails
    let input: DeriveInput = parse_quote! {
        struct Test {
            #[column(min = 5)]
            field: i32,
        }
    };
    let result = ParsedStruct::from_derive_input(input);
    assert!(result.is_err());
    // The error message comes from darling
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("Unknown option 'min'"), "Msg was: {}", msg);
}

#[test]
fn test_cross_attribute_pollution_column_in_validate() {
    // Test that putting a column attribute (primary_key) in validate fails
    let input: DeriveInput = parse_quote! {
        struct Test {
            #[validate(primary_key)]
            field: i32,
        }
    };
    let result = ParsedStruct::from_derive_input(input);
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("Unknown option 'primary_key'"), "Msg was: {}", msg);
}

#[test]
fn test_cross_attribute_pollution_field_in_validate() {
    // Test that putting a field attribute (flatten) in validate fails
    let input: DeriveInput = parse_quote! {
        struct Test {
            #[validate(flatten)]
            field: i32,
        }
    };
    let result = ParsedStruct::from_derive_input(input);
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("Unknown option 'flatten'"), "Msg was: {}", msg);
}

#[test]
fn test_doc_comment_extraction() {
    let input: DeriveInput = parse_quote! {
        /// This is a doc comment
        /// Second line
        struct Test {
            field: i32
        }
    };
    let attrs = input.attrs;
    let doc = extract_doc_comment(&attrs).unwrap();
    assert_eq!(doc, "This is a doc comment\n Second line");
}

#[test]
fn test_column_attrs_invalid_field() {
    let input: DeriveInput = parse_quote! {
        struct Test {
            #[column(min = 90)]
            discount: f64,
        }
    };
    let err = ParsedStruct::from_derive_input(input).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Unknown option 'min' "), "Msg was: {}", msg);
}
