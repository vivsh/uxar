use crate::schemables::{Schemable, SchemaType};

// Integer types
impl Schemable for i8 {
    fn schema_type() -> SchemaType {
        SchemaType::Int { bits: 8 }
    }
}

impl Schemable for i16 {
    fn schema_type() -> SchemaType {
        SchemaType::Int { bits: 16 }
    }
}

impl Schemable for i32 {
    fn schema_type() -> SchemaType {
        SchemaType::Int { bits: 32 }
    }
}

impl Schemable for i64 {
    fn schema_type() -> SchemaType {
        SchemaType::Int { bits: 64 }
    }
}

impl Schemable for u8 {
    fn schema_type() -> SchemaType {
        SchemaType::Int { bits: 8 }
    }
}

impl Schemable for u16 {
    fn schema_type() -> SchemaType {
        SchemaType::Int { bits: 16 }
    }
}

impl Schemable for u32 {
    fn schema_type() -> SchemaType {
        SchemaType::Int { bits: 32 }
    }
}

impl Schemable for u64 {
    fn schema_type() -> SchemaType {
        SchemaType::Int { bits: 64 }
    }
}

// Float types
impl Schemable for f32 {
    fn schema_type() -> SchemaType {
        SchemaType::Float { bits: 32 }
    }
}

impl Schemable for f64 {
    fn schema_type() -> SchemaType {
        SchemaType::Float { bits: 64 }
    }
}

// Bool
impl Schemable for bool {
    fn schema_type() -> SchemaType {
        SchemaType::Bool
    }
}

// String types
impl Schemable for String {
    fn schema_type() -> SchemaType {
        SchemaType::Str { width: None }
    }
}

impl Schemable for &str {
    fn schema_type() -> SchemaType {
        SchemaType::Str { width: None }
    }
}

// Option<T>
impl<T: Schemable> Schemable for Option<T> {
    fn schema_type() -> SchemaType {
        SchemaType::Optional {
            inner: Box::new(T::schema_type()),
        }
    }
}

// Vec<T>
impl<T: Schemable> Schemable for Vec<T> {
    fn schema_type() -> SchemaType {
        SchemaType::List {
            item: Box::new(T::schema_type()),
        }
    }
}

// HashMap<String, T>
impl<T: Schemable> Schemable for std::collections::HashMap<String, T> {
    fn schema_type() -> SchemaType {
        SchemaType::Map {
            value: Box::new(T::schema_type()),
        }
    }
}

// BTreeMap<String, T>
impl<T: Schemable> Schemable for std::collections::BTreeMap<String, T> {
    fn schema_type() -> SchemaType {
        SchemaType::Map {
            value: Box::new(T::schema_type()),
        }
    }
}
