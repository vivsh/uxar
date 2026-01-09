use std::vec;

use axum::{
    Json,
    extract::{Path, Query},
    response::Html,
};

use crate::{roles::{self, BitRole, HasPerm, Permit, RoleType}, schemables::{SchemaType, Schemable, schema::IntoApiSchema}, views::{ParamMeta, ViewMeta}};



#[derive(Debug, thiserror::Error)]
pub enum FragmentError {
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
pub enum ApiFragment {
    Header(SchemaType),

    Cookie(SchemaType),

    Query(SchemaType),

    Path(SchemaType),
    /// To be used for request and responsebodies
    /// First argument is the schema, second is the content type
    ///  Third optional argument is status code for response bodies
    // Overloading is used because many structs will be used both as request and response bodies
    // Having a status and content_type is also essential for supporting
    // (status_code, body) or (status_code, body, content_type) return types
    Body(SchemaType, String, Option<u16>),

    Security {
        scheme: String,
        scopes: Vec<String>,
        join_all: bool,
    },
}

pub trait IntoApiParts {
    fn api_parts() -> Vec<ApiFragment>;
}

#[derive(Debug, Clone, Default)]
pub struct Nil<T = ()>(std::marker::PhantomData<T>);


impl<T> IntoApiParts for Nil<T> {
    fn api_parts() -> Vec<ApiFragment> {
        vec![]
    }
}

impl IntoApiParts for () {
    fn api_parts() -> Vec<ApiFragment> {
        vec![]
    }
}

impl<T> IntoApiParts for Query<T> where T: Schemable {
    fn api_parts() -> Vec<ApiFragment> {
        vec![ApiFragment::Query(T::schema_type())]
    }
}

impl<T> IntoApiParts for Path<T> where T: Schemable {
    fn api_parts() -> Vec<ApiFragment> {
        vec![ApiFragment::Path(T::schema_type())]
    }
}

impl<T> IntoApiParts for Json<T> where T: Schemable {
    fn api_parts() -> Vec<ApiFragment> {
        vec![ApiFragment::Body(
            T::schema_type(),
            "application/json".to_string(),
            None,
        )]
    }
}

impl<T> IntoApiParts for Html<T> where T: Schemable {
    fn api_parts() -> Vec<ApiFragment> {
        vec![ApiFragment::Body(
            T::schema_type(),
            "text/html".to_string(),
            None,
        )]
    }
}

impl<'a> IntoApiParts for &'a str {
    fn api_parts() -> Vec<ApiFragment> {
        vec![ApiFragment::Body(
            SchemaType::Str { width: None },
            "text/plain".to_string(),
            None,
        )]
    }
}


impl <const N: RoleType, R: BitRole, O: HasPerm> IntoApiParts for Permit<N, R, O> {
    
    fn api_parts() -> Vec<ApiFragment> {
        let roles = crate::roles::format_roles::<R>(N);        
        vec![ApiFragment::Security {
            scheme: "bearerAuth".to_string(),
            scopes: roles,
            join_all: O::join_all(),
        }]
    }

}
