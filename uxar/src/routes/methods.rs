use std::ops::{BitOr, BitOrAssign};
use std::ops::Deref;

use axum::routing::MethodFilter;

/// HTTP methods a handler accepts. Supports combining methods with `|` operator.
///
/// # Examples
///
/// ```ignore
/// use uxar::routes::Methods;
///
/// // Single method
/// let get = Methods::GET;
///
/// // Combine methods with | operator
/// let methods = Methods::GET | Methods::POST | Methods::PUT;
///
/// // Iterate over methods
/// for (name, _) in methods.iter() {
///     println!("Accepts: {}", name);
/// }
///
/// // Convert single method to string
/// assert_eq!(Methods::GET.to_str(), Some("GET"));
///
/// // Get all method names
/// assert_eq!(methods.to_vec(), vec!["GET", "POST", "PUT"]);
/// ```
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Methods(MethodFilter);

impl serde::Serialize for Methods {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let methods = self.to_vec();
        let mut seq = serializer.serialize_seq(Some(methods.len()))?;
        for method in methods {
            seq.serialize_element(method)?;
        }
        seq.end()
    }
}

impl<'de> serde::Deserialize<'de> for Methods {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct MethodsVisitor;

        impl<'de> serde::de::Visitor<'de> for MethodsVisitor {
            type Value = Methods;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or list of HTTP method names")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Methods::from_str(v)
                    .ok_or_else(|| E::custom(format!("unknown HTTP method: {}", v)))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut methods: Option<Methods> = None;
                while let Some(method_str) = seq.next_element::<String>()? {
                    let method = Methods::from_str(&method_str).ok_or_else(|| {
                        serde::de::Error::custom(format!("unknown HTTP method: {}", method_str))
                    })?;
                    methods = Some(methods.map_or(method, |m| m | method));
                }
                methods.ok_or_else(|| serde::de::Error::custom("empty method list"))
            }
        }

        deserializer.deserialize_any(MethodsVisitor)
    }
}

impl Methods {
    /// Match `GET` requests.
    pub const GET: Self = Self(MethodFilter::GET);
    /// Match `POST` requests.
    pub const POST: Self = Self(MethodFilter::POST);
    /// Match `PUT` requests.
    pub const PUT: Self = Self(MethodFilter::PUT);
    /// Match `PATCH` requests.
    pub const PATCH: Self = Self(MethodFilter::PATCH);
    /// Match `DELETE` requests.
    pub const DELETE: Self = Self(MethodFilter::DELETE);
    /// Match `HEAD` requests.
    pub const HEAD: Self = Self(MethodFilter::HEAD);
    /// Match `OPTIONS` requests.
    pub const OPTIONS: Self = Self(MethodFilter::OPTIONS);
    /// Match `TRACE` requests.
    pub const TRACE: Self = Self(MethodFilter::TRACE);
    /// Match `CONNECT` requests.
    pub const CONNECT: Self = Self(MethodFilter::CONNECT);

    /// Iterate over individual methods in this filter.
    pub fn iter(&self) -> MethodIter {
        MethodIter { filter: self.0, position: 0 }
    }

    /// Combine with another method filter (same as `|` operator).
    pub fn or(self, other: Self) -> Self {
        Self(self.0.or(other.0))
    }

    /// Convert to string if this is a single method, None for combined.
    pub fn to_str(&self) -> Option<&'static str> {
        KNOWN_METHODS.iter()
            .find(|(_, f)| *f == self.0)
            .map(|(name, _)| *name)
    }

    /// Convert to vec of all method names in this filter.
    pub fn to_vec(&self) -> Vec<&'static str> {
        self.iter().map(|(name, _)| name).collect()
    }

    /// Parse a method string (case-insensitive).
    pub fn from_str(s: &str) -> Option<Self> {
        let upper = s.to_uppercase();
        KNOWN_METHODS.iter()
            .find(|(name, _)| *name == upper.as_str())
            .map(|(_, filter)| Self(*filter))
    }
}

impl BitOr for Methods {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0.or(rhs.0))
    }
}

impl BitOrAssign for Methods {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 = self.0.or(rhs.0);
    }
}

impl From<Methods> for MethodFilter {
    fn from(methods: Methods) -> Self {
        methods.0
    }
}

impl From<MethodFilter> for Methods {
    fn from(filter: MethodFilter) -> Self {
        Self(filter)
    }
}

impl Deref for Methods {
    type Target = MethodFilter;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Iterator over individual methods.
pub struct MethodIter {
    filter: MethodFilter,
    position: usize,
}

impl Iterator for MethodIter {
    type Item = (&'static str, Methods);

    fn next(&mut self) -> Option<Self::Item> {
        while self.position < KNOWN_METHODS.len() {
            let (name, method) = KNOWN_METHODS[self.position];
            self.position += 1;
            let combined = self.filter.or(method);
            if combined == self.filter {
                return Some((name, Methods(method)));
            }
        }
        None
    }
}

const KNOWN_METHODS: &[(&str, MethodFilter)] = &[
    ("GET", MethodFilter::GET),
    ("POST", MethodFilter::POST),
    ("PUT", MethodFilter::PUT),
    ("PATCH", MethodFilter::PATCH),
    ("DELETE", MethodFilter::DELETE),
    ("HEAD", MethodFilter::HEAD),
    ("OPTIONS", MethodFilter::OPTIONS),
    ("TRACE", MethodFilter::TRACE),
    ("CONNECT", MethodFilter::CONNECT),
];
