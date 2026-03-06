use std::{sync::Arc, vec};

use axum::{
    Json,
    extract::{Path, Query},
    response::Html,
};
use axum_extra::extract::Query as QueryExtra;
use schemars::JsonSchema;

use crate::{AuthUser, roles::{self, BitRole, HasPerm, Permit, RoleType}};

#[derive(Clone)]
pub struct LazySchema{
    converter: fn(&mut schemars::SchemaGenerator) -> schemars::Schema,
}

impl std::fmt::Debug for LazySchema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LazySchema").finish()
    }
}

impl LazySchema {

    pub fn wrap<T: JsonSchema>() -> Self
    where        
    {
        fn converter<T: JsonSchema>(genr: &mut schemars::SchemaGenerator) -> schemars::Schema {
            genr.subschema_for::<T>()
        }
        Self {
            converter: converter::<T>,
        }
    }

    pub fn generate(&self, genr: &mut schemars::SchemaGenerator) -> schemars::Schema {
        (self.converter)(genr)
    }

}

#[derive(Debug, thiserror::Error)]
pub enum ApiPartError {
    #[error("Invalid parameter fragment {fragment} for {param} used in operation {route} with url {url}")]
    InvalidParameter {
        route: String,
        url: String,
        param: String, 
        fragment: String,
    },
    #[error("Invalid return fragment {fragment} used in operation {route} with url {url}")]
    InvalidReturn{
        route: String,
        url: String,
        fragment: String,
    }
}


#[derive(Debug, Clone)]
pub enum ApiPart {
    Header(LazySchema),

    Cookie(LazySchema),

    Query(LazySchema),

    Path(LazySchema),
    /// To be used for request and responsebodies
    /// First argument is the schema, second is the content type
    ///  Third optional argument is status code for response bodies
    // Overloading is used because many structs will be used both as request and response bodies
    // Having a status and content_type is also essential for supporting
    // (status_code, body) or (status_code, body, content_type) return types
    Body(LazySchema, String, Option<u16>),

    Security {
        scheme: String,
        scopes: Vec<String>,
        join_all: bool,
    },
}

pub trait IntoApiParts {
    fn api_parts() -> Vec<ApiPart>;
}

#[derive(Debug, Clone, Default)]
pub struct Nil<T = ()>(std::marker::PhantomData<T>);


impl<T> IntoApiParts for Nil<T> {
    fn api_parts() -> Vec<ApiPart> {
        vec![]
    }
}

impl IntoApiParts for () {
    fn api_parts() -> Vec<ApiPart> {
        vec![]
    }
}

impl<T> IntoApiParts for Query<T> where T: JsonSchema {
    fn api_parts() -> Vec<ApiPart> {
        vec![ApiPart::Query(LazySchema::wrap::<T>())]
    }
}

impl<T> IntoApiParts for QueryExtra<T> where T: JsonSchema {
    fn api_parts() -> Vec<ApiPart> {
        vec![ApiPart::Query(LazySchema::wrap::<T>())]
    }
}

impl<T> IntoApiParts for Path<T> where T: JsonSchema {
    fn api_parts() -> Vec<ApiPart> {
        vec![ApiPart::Path(LazySchema::wrap::<T>())]
    }
}

impl<T> IntoApiParts for Json<T> where T: JsonSchema {
    fn api_parts() -> Vec<ApiPart> {
        vec![ApiPart::Body(
            LazySchema::wrap::<T>(),
            "application/json".to_string(),
            None,
        )]
    }
}

impl IntoApiParts for axum::http::StatusCode {
    fn api_parts() -> Vec<ApiPart> {
        vec![
            ApiPart::Body(
                LazySchema::wrap::<()>(),
                "text/plain".to_string(),
                Some(axum::http::StatusCode::OK.as_u16()),
            ),
        ]
    }
}


impl<T> IntoApiParts for Html<T> where T: JsonSchema {
    fn api_parts() -> Vec<ApiPart> {
        vec![ApiPart::Body(
            LazySchema::wrap::<T>(),
            "text/html".to_string(),
            None,
        )]
    }
}

impl<'a> IntoApiParts for &'a str {
    fn api_parts() -> Vec<ApiPart> {
        vec![ApiPart::Body(
            LazySchema::wrap::<String>(),
            "text/plain".to_string(),
            None,
        )]
    }
}


impl <const N: RoleType, R: BitRole, O: HasPerm> IntoApiParts for Permit<N, R, O> {
    
    fn api_parts() -> Vec<ApiPart> {
        let roles = crate::roles::format_roles::<R>(N);        
        vec![ApiPart::Security {
            scheme: "bearerAuth".to_string(),
            scopes: roles,
            join_all: O::join_all(),
        }]
    }

}

impl IntoApiParts for AuthUser {
    fn api_parts() -> Vec<ApiPart> {
        let roles = Vec::new();        
        vec![ApiPart::Security {
            scheme: "bearerAuth".to_string(),
            scopes: roles,
            join_all: false,
        }]
    }
}
