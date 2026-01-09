
use std::collections::HashSet;

use crate::schemables::{SchemaType, SchemaConstraints, ScalarLit, StringFormat};
use openapiv3::{Schema, SchemaKind, SchemaData, Type, StringType, IntegerType, NumberType, 
                 ArrayType, ObjectType, ReferenceOr};
use indexmap::IndexMap;

/// Registry for OpenAPI schema components to enable deduplication via refs.
#[derive(Debug, Default)]
pub struct ComponentRegistry {
    components: IndexMap<String, openapiv3::Schema>,
    security_schemes: HashSet<String>,
    operation_scopes: HashSet<String>,
    pub (crate) operation_scope_join_all: bool,
    operation_security: HashSet<String>,
}

impl ComponentRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            components: IndexMap::new(),
            security_schemes: HashSet::new(),
            operation_scopes: HashSet::new(),
            operation_security: HashSet::new(),
            operation_scope_join_all: false,
        }
    }

    /// Register a schema with a given name. Returns the reference path.
    pub fn register(&mut self, name: String, schema: openapiv3::Schema) -> String {
        let ref_path = format!("#/components/schemas/{}", name);
        self.components.insert(name, schema);
        ref_path
    }

    pub fn register_security(&mut self, name: String, scopes: &[String], join_all: bool) {
        // self.security_schemes.insert(name.clone());
        self.operation_security.insert(name);
        self.operation_scope_join_all = join_all;
        self.operation_scopes.extend(scopes.iter().cloned());

    }

    pub fn drain_operation_scopes(&mut self) -> impl Iterator<Item=String> + '_ {
        self.operation_scopes.drain()
    }

    pub fn drain_operation_security(&mut self) -> impl Iterator<Item=String> + '_ {
        self.operation_security.drain()
    }

    /// Check if a schema name is already registered.
    pub fn contains(&self, name: &str) -> bool {
        self.components.contains_key(name)
    }

    /// Consume the registry and return the component schemas map.
    pub fn into_components(self) -> IndexMap<String, openapiv3::ReferenceOr<openapiv3::Schema>> {
        self.components
            .into_iter()
            .map(|(k, v)| (k, openapiv3::ReferenceOr::Item(v)))
            .collect()
    }
}


pub trait IntoApiSchema{
    fn into_api_schema(self, registry: &mut ComponentRegistry) -> ReferenceOr<openapiv3::Schema>;
}

impl IntoApiSchema for openapiv3::Schema{
    fn into_api_schema(self, _registry: &mut ComponentRegistry) -> ReferenceOr<openapiv3::Schema> {
        // Schemas passed directly are wrapped as items
        ReferenceOr::Item(self)
    }
}

impl IntoApiSchema for SchemaType {
    fn into_api_schema(self, registry: &mut ComponentRegistry) -> ReferenceOr<openapiv3::Schema> {
        schema_type_to_api_schema(&self, registry)
    }
}

pub fn schema_type_to_api_schema(st: &SchemaType, registry: &mut ComponentRegistry) -> ReferenceOr<Schema> {
    match st {
        SchemaType::Int { bits } => {
            let format = match bits {
                8 | 16 | 32 => openapiv3::VariantOrUnknownOrEmpty::Item(
                    openapiv3::IntegerFormat::Int32
                ),
                _ => openapiv3::VariantOrUnknownOrEmpty::Item(
                    openapiv3::IntegerFormat::Int64
                ),
            };
            ReferenceOr::Item(Schema {
                schema_data: SchemaData::default(),
                schema_kind: SchemaKind::Type(Type::Integer(IntegerType {
                    format,
                    ..Default::default()
                })),
            })
        }
        SchemaType::Str { width: _ } => {
            ReferenceOr::Item(Schema {
                schema_data: SchemaData::default(),
                schema_kind: SchemaKind::Type(Type::String(StringType::default())),
            })
        }
        SchemaType::Bool => {
            ReferenceOr::Item(Schema {
                schema_data: SchemaData::default(),
                schema_kind: SchemaKind::Type(Type::Boolean(Default::default())),
            })
        }
        SchemaType::Float { bits } => {
            let format = if *bits == 32 {
                openapiv3::VariantOrUnknownOrEmpty::Item(openapiv3::NumberFormat::Float)
            } else {
                openapiv3::VariantOrUnknownOrEmpty::Item(openapiv3::NumberFormat::Double)
            };
            ReferenceOr::Item(Schema {
                schema_data: SchemaData::default(),
                schema_kind: SchemaKind::Type(Type::Number(NumberType {
                    format,
                    ..Default::default()
                })),
            })
        }
        SchemaType::Optional { inner } => {
            let inner_ref = schema_type_to_api_schema(inner, registry);
            // Note: OpenAPI 3.0 uses nullable: true. OpenAPI 3.1 would use type unions
            // (anyOf with null), but the openapiv3 crate v2.2.0 only supports 3.0.
            match inner_ref {
                ReferenceOr::Item(mut schema) => {
                    schema.schema_data.nullable = true;
                    ReferenceOr::Item(schema)
                }
                ReferenceOr::Reference { reference } => {
                    // For refs, wrap in allOf with nullable to preserve the $ref
                    ReferenceOr::Item(Schema {
                        schema_data: SchemaData {
                            nullable: true,
                            ..Default::default()
                        },
                        schema_kind: SchemaKind::AllOf {
                            all_of: vec![ReferenceOr::Reference { reference }],
                        },
                    })
                }
            }
        }
        SchemaType::List { item } => {
            let item_schema = schema_type_to_api_schema(item, registry);
            let items_ref = match item_schema {
                ReferenceOr::Item(s) => ReferenceOr::Item(Box::new(s)),
                ReferenceOr::Reference { reference } => ReferenceOr::Reference { reference },
            };
            ReferenceOr::Item(Schema {
                schema_data: SchemaData::default(),
                schema_kind: SchemaKind::Type(Type::Array(ArrayType {
                    items: Some(items_ref),
                    min_items: None,
                    max_items: None,
                    unique_items: false,
                })),
            })
        }
        SchemaType::Map { value } => {
            let value_schema = schema_type_to_api_schema(value, registry);
            ReferenceOr::Item(Schema {
                schema_data: SchemaData::default(),
                schema_kind: SchemaKind::Type(Type::Object(ObjectType {
                    properties: Default::default(),
                    required: Vec::new(),
                    additional_properties: Some(openapiv3::AdditionalProperties::Schema(
                        Box::new(value_schema)
                    )),
                    min_properties: None,
                    max_properties: None,
                })),
            })
        }
        SchemaType::Struct (schema) => {
            let type_name = schema.name.to_string();
            if !registry.contains(&type_name) {
                let openapi_schema = struct_to_openapi(
                    &schema.name,
                    &schema.fields,
                    schema.about.as_ref().map(|s| s.as_ref()),
                    &schema.tags,
                    registry,
                );
                registry.register(type_name.clone(), openapi_schema);
            }
            ReferenceOr::Reference {
                reference: format!("#/components/schemas/{}", type_name),
            }
        }
    }
}

fn struct_to_openapi(
    name: &str,
    fields: &[crate::schemables::SchemaField],
    about: Option<&str>,
    _tags: &[std::borrow::Cow<'static, str>],
    registry: &mut ComponentRegistry,
) -> Schema {
    let mut properties = IndexMap::new();
    let mut required = Vec::new();

    for field in fields {
        if field.skip == Some(true) {
            continue;
        }

        let field_schema_ref = schema_type_to_api_schema(&field.schema_type, registry);
        let mut field_schema = match field_schema_ref {
            ReferenceOr::Item(s) => s,
            ReferenceOr::Reference { reference } => {
                // Keep refs as-is
                properties.insert(field.name.to_string(), ReferenceOr::Reference { reference });
                
                let is_optional = matches!(field.schema_type, SchemaType::Optional { .. });
                if !is_optional {
                    required.push(field.name.to_string());
                }
                continue;
            }
        };
        
        apply_constraints(&mut field_schema, &field.constraints);
        
        if let Some(desc) = &field.about {
            field_schema.schema_data.description = Some(desc.to_string());
        }

        let is_optional = matches!(field.schema_type, SchemaType::Optional { .. });
        if !is_optional {
            required.push(field.name.to_string());
        }

        properties.insert(field.name.to_string(), ReferenceOr::Item(Box::new(field_schema)));
    }

    Schema {
        schema_data: SchemaData {
            title: Some(name.to_string()),
            description: about.map(|s| s.to_string()),
            ..Default::default()
        },
        schema_kind: SchemaKind::Type(Type::Object(ObjectType {
            properties,
            required,
            additional_properties: None,
            min_properties: None,
            max_properties: None,
        })),
    }
}

fn apply_constraints(schema: &mut Schema, constraints: &SchemaConstraints) {
    match &mut schema.schema_kind {
        SchemaKind::Type(Type::String(string_type)) => {
            if !constraints.enumeration.is_empty() {
                string_type.enumeration = constraints.enumeration.iter()
                    .map(scalar_to_string)
                    .collect();
            }
            if let Some(min_len) = constraints.min_length {
                string_type.min_length = Some(min_len);
            }
            if let Some(max_len) = constraints.max_length {
                string_type.max_length = Some(max_len);
            }
            if let Some(pattern) = &constraints.pattern {
                string_type.pattern = Some(pattern.to_string());
            }
            if let Some(format) = constraints.format {
                string_type.format = openapiv3::VariantOrUnknownOrEmpty::Unknown(
                    string_format_to_openapi(format).to_string()
                );
            }
        }
        SchemaKind::Type(Type::Integer(int_type)) => {
            if !constraints.enumeration.is_empty() {
                int_type.enumeration = constraints.enumeration.iter()
                    .filter_map(|lit| scalar_to_i64(*lit))
                    .map(Some)
                    .collect();
            }
            if let Some(min) = constraints.minimum {
                int_type.minimum = scalar_to_i64(min);
                int_type.exclusive_minimum = constraints.exclusive_minimum;
            }
            if let Some(max) = constraints.maximum {
                int_type.maximum = scalar_to_i64(max);
                int_type.exclusive_maximum = constraints.exclusive_maximum;
            }
            if let Some(mult) = constraints.multiple_of {
                int_type.multiple_of = scalar_to_i64(mult);
            }
        }
        SchemaKind::Type(Type::Number(num_type)) => {
            if !constraints.enumeration.is_empty() {
                num_type.enumeration = constraints.enumeration.iter()
                    .filter_map(|lit| scalar_to_f64(*lit))
                    .map(Some)
                    .collect();
            }
            if let Some(min) = constraints.minimum {
                num_type.minimum = scalar_to_f64(min);
                num_type.exclusive_minimum = constraints.exclusive_minimum;
            }
            if let Some(max) = constraints.maximum {
                num_type.maximum = scalar_to_f64(max);
                num_type.exclusive_maximum = constraints.exclusive_maximum;
            }
            if let Some(mult) = constraints.multiple_of {
                num_type.multiple_of = scalar_to_f64(mult);
            }
        }
        SchemaKind::Type(Type::Array(arr_type)) => {
            if let Some(min_items) = constraints.min_items {
                arr_type.min_items = Some(min_items);
            }
            if let Some(max_items) = constraints.max_items {
                arr_type.max_items = Some(max_items);
            }
            arr_type.unique_items = constraints.unique_items;
        }
        _ => {}
    }
}

fn string_format_to_openapi(format: StringFormat) -> &'static str {
    match format {
        StringFormat::Email => "email",
        StringFormat::PhoneE164 => "phone",
        StringFormat::Url => "uri",
        StringFormat::Uuid => "uuid",
        StringFormat::Date => "date",
        StringFormat::DateTime => "date-time",
        StringFormat::IpV4 => "ipv4",
        StringFormat::IpV6 => "ipv6",
    }
}

fn scalar_lit_to_json(lit: &ScalarLit) -> serde_json::Value {
    match lit {
        ScalarLit::Bool(b) => serde_json::Value::Bool(*b),
        ScalarLit::Int(i) => {
            if let Ok(val) = i64::try_from(*i) {
                serde_json::Value::Number(val.into())
            } else {
                serde_json::Value::Null
            }
        }
        ScalarLit::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        ScalarLit::Str(s) => serde_json::Value::String(s.to_string()),
    }
}

fn scalar_to_string(lit: &ScalarLit) -> Option<String> {
    match lit {
        ScalarLit::Str(s) => Some(s.to_string()),
        ScalarLit::Bool(b) => Some(b.to_string()),
        ScalarLit::Int(i) => Some(i.to_string()),
        ScalarLit::Float(f) => Some(f.to_string()),
    }
}

fn scalar_to_i64(lit: ScalarLit) -> Option<i64> {
    match lit {
        ScalarLit::Int(i) => i.try_into().ok(),
        _ => None,
    }
}

fn scalar_to_f64(lit: ScalarLit) -> Option<f64> {
    match lit {
        ScalarLit::Int(i) => Some(i as f64),
        ScalarLit::Float(f) => Some(f),
        _ => None,
    }
}