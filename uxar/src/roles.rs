// roles.rs

use axum::{extract::FromRequestParts, http::request::Parts, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, marker::PhantomData};
use strum::IntoEnumIterator;
use axum::response::{IntoResponse, Response};
use strum_macros::EnumIter;

// ---- TRAIT ----
pub trait BitRole: Copy + IntoEnumIterator + Debug + 'static {
    fn as_usize(self) -> usize;
}

// ---- PERMISSION CHECK ----
pub const fn has_permission(mask: usize, role: usize) -> bool {
    if role >= usize::BITS as usize {
        return false;
    }
    mask & (1 << role) != 0
}

// ---- FORMATTING ----
pub fn format_roles<R: BitRole>(mask: usize) -> Vec<String> {
    let mut roles = R::iter()
        .filter(|r| (mask & (1 << r.as_usize())) != 0)
        .map(|r| format!("{:?}", r))
        .collect::<Vec<_>>();
    roles.sort();
    roles
}

// ---- AUTH USER ----
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub id: i64,
    pub kind: i64,
    pub is_staff: bool,
}

// ---- AUTH ERROR ----
#[derive(Debug)]
pub enum AuthError {
    Forbidden,
    MissingToken,
    InvalidToken,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AuthError::Forbidden => (StatusCode::FORBIDDEN, "Permission denied"),
            AuthError::MissingToken => (StatusCode::UNAUTHORIZED, "Missing token"),
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid token"),
        };
        (status, msg).into_response()
    }
}

pub struct Permit<const MASK: usize, R: BitRole>(pub AuthUser, PhantomData<R>);

impl<const MASK: usize, R: BitRole> Permit<MASK, R> {
    pub fn describe() -> String {
        format_roles::<R>(MASK).join(", ")
    }

    pub fn into_user(self) -> AuthUser {
        self.0
    }
}

impl<const MASK: usize, R: BitRole> FromRequestParts<()> for Permit<MASK, R> {
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &()) -> Result<Self, Self::Rejection> {
        let user = parts
            .extensions
            .get::<AuthUser>()
            .cloned()
            .ok_or(AuthError::MissingToken)?;

        if !has_permission(MASK, user.kind as usize) {
            return Err(AuthError::Forbidden);
        }

        Ok(Permit(user, PhantomData))
    }
}

// ---- MACRO ----
#[macro_export]
macro_rules! permit {
    ($role_ty:ty, $($role:ident)|+ $(,)?) => {
        $crate::roles::Permit::<{
            0 $(| (1 << <$role_ty>::$role as usize))+
        }, $role_ty>
    };
}

// ---- EXAMPLE ENUM ----
#[derive(Debug, Clone, Copy, EnumIter)]
#[repr(usize)]
pub enum Role {
    Admin = 0,
    Editor = 1,
    Viewer = 2,
}

impl BitRole for Role {
    fn as_usize(self) -> usize {
        self as usize
    }
}