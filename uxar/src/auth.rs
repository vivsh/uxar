
use super::site::Site;
use axum::http::request::Parts;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{self, Cookie};
use axum_extra::extract::CookieJar;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Validation};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use time;



#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CookieConf {
    pub name: String,
    pub path: String,
    pub http_only: bool,
    pub secure: bool,
    pub same_site: String,
}

impl Default for CookieConf {
    fn default() -> Self {
        return CookieConf {
            name: "".to_string(),
            path: "/".to_string(),
            http_only: true,
            secure: true,
            same_site: "Lax".to_string(),
        };
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthConf {
    pub access_ttl: i64,
    pub refresh_ttl: i64,
    pub access_cookie: Option<CookieConf>,
    pub refresh_cookie: Option<CookieConf>,
}

impl Default for AuthConf {
    fn default() -> Self {
        return AuthConf {
            access_ttl: 3600,
            refresh_ttl: 604800,
            access_cookie: Some(CookieConf{
                name: "access_token".to_string(),
                ..Default::default()
            }),
            refresh_cookie: Some(CookieConf{
                name: "refresh_token".to_string(),
                same_site: "Strict".to_string(),
                ..Default::default()
            }),
        };
    }
}


fn extract_token(parts: &Parts) -> Option<&str> {
    let header = parts.headers.get(header::AUTHORIZATION)?;
    let value = header.as_bytes();
    if value.len() > 4 && value[..4].eq_ignore_ascii_case(b"JWT ") {
        std::str::from_utf8(&value[4..]).ok()
    } else {
        None
    }
}

fn unix_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenPair{
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Clone)]
pub struct Authenticator {
    access_ttl: i64,
    refresh_ttl: i64,    
    access_cookie_conf: Option<CookieConf>,
    refresh_cookie_conf: Option<CookieConf>,
    access_cookie_same_site: cookie::SameSite,
    refresh_cookie_same_site: cookie::SameSite,
    algorithm: Algorithm,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    validation: Validation,
}

impl std::fmt::Debug for Authenticator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Authenticator")
            .field("access_ttl", &self.access_ttl)
            .field("refresh_ttl", &self.refresh_ttl)
            .field("access_cookie_conf", &self.access_cookie_conf)
            .field("refresh_cookie_conf", &self.refresh_cookie_conf)
            .field("algorithm", &self.algorithm)
            .finish()
    }
}

fn get_cookie_same_site(cookie_conf: &Option<CookieConf>) -> cookie::SameSite {
    if let Some(conf) = cookie_conf {
        match conf.same_site.to_lowercase().as_str() {
            "lax" => cookie::SameSite::Lax,
            "strict" => cookie::SameSite::Strict,
            "none" => cookie::SameSite::None,
            _ => cookie::SameSite::Lax,
        }
    } else {
        cookie::SameSite::Lax
    }
}

impl Authenticator {

    pub(crate) fn new(
        conf: &AuthConf,
        secret_key: &str,
    ) -> Self {
        let secret_key = secret_key.as_bytes();
        let access_ttl = conf.access_ttl;
        let refresh_ttl = conf.refresh_ttl;
        let access_cookie_conf = conf.access_cookie.clone();
        let refresh_cookie_conf = conf.refresh_cookie.clone();
        let algorithm = Algorithm::HS256;
        let encoding_key = EncodingKey::from_secret(secret_key);
        let decoding_key = DecodingKey::from_secret(secret_key);
        let validation = Validation::new(algorithm);
        
        Self {
            access_ttl,
            refresh_ttl,
            access_cookie_same_site: get_cookie_same_site(&access_cookie_conf),
            refresh_cookie_same_site: get_cookie_same_site(&refresh_cookie_conf),
            access_cookie_conf,
            refresh_cookie_conf,
            algorithm,
            encoding_key,
            decoding_key,
            validation,
        }
    }

    pub fn encode(&self, item: &JWTClaim) -> Result<String, AuthError> {
        let key = &self.encoding_key;
        let header = jsonwebtoken::Header::new(self.algorithm);
        encode(&header, item, &key).map_err(|e| AuthError::from(&e))
    }

    pub fn decode(&self, token: &str) -> Result<JWTClaim, AuthError> {
        let key = &self.decoding_key;
        decode::<JWTClaim>(&token, &key, &self.validation)
            .map(|o| o.claims)
            .map_err(|e| AuthError::from(&e))
    }

    pub fn extract_claims(&self, parts: &Parts, refresh: bool) -> Result<JWTClaim, AuthError> {
        let cookies_conf = if refresh{
            &self.refresh_cookie_conf
        } else {
            &self.access_cookie_conf    
        };
        let token = extract_token(parts)
            .map(|t| t.to_owned())
            .or_else(|| {
                cookies_conf
                    .as_ref()
                    .map(|c| c.name.as_str())
                    .and_then(|cookie_name| {
                        CookieJar::from_headers(&parts.headers)
                            .get(cookie_name)
                            .map(|c| c.value().to_owned())
                    })
            })
            .ok_or(AuthError::MissingToken)?;
        let claims = self.decode(&token)?;        
        return Ok(claims);
    }

    pub fn extract_user(&self, parts: &Parts, aud: &[&str], refresh: bool) -> Result<AuthUser, AuthError> {
        let claims = self.extract_claims(parts, refresh)?;
        if !claims.aud.is_empty() && !aud.is_empty() {
            if !claims.aud.iter().any(|a| {aud.iter().any(|&b| a == b)}) {
                return Err(AuthError::Forbidden);
            }
        }
        let user = claims.into_auth_user();
        return Ok(user);
    }

    pub fn create_token_pair(&self, user: AuthUser, aud: &[&str]) -> Result<TokenPair, AuthError> {
        let aud: Vec<String> = aud.iter().map(|&s| s.to_string()).collect();
        let access_claims = JWTClaim::new(&user, "", aud.clone(), self.access_ttl);
        let access_token = self.encode(&access_claims)?;
        let mut refresh_claims = JWTClaim::new(&user, "", aud, self.refresh_ttl);
        refresh_claims.refresh = true;
        let refresh_token = self.encode(&refresh_claims)?;
        Ok(TokenPair { access_token, refresh_token})
    }

    pub fn login(&self, token: &str, refresh: bool, resp: &mut Response) {
        let cookie_conf = if refresh {
            &self.refresh_cookie_conf
        } else {
            &self.access_cookie_conf
        };
        let same_site = if refresh {
            self.refresh_cookie_same_site
        } else {
            self.access_cookie_same_site
        };
        let access_ttl = if refresh {
            self.refresh_ttl
        } else {
            self.access_ttl
        };
        if let Some(conf) = cookie_conf {
            let c = Cookie::build((&conf.name, token))
                .path(conf.path.as_str())
                .max_age(time::Duration::seconds(access_ttl))
                .http_only(conf.http_only)
                .same_site(same_site)
                .secure(conf.secure)
                .build();

            match c.to_string().parse() {
                Ok(hv) => {
                    resp.headers_mut().append(header::SET_COOKIE, hv);
                }
                Err(err) => {
                    *resp.status_mut() = axum::http::StatusCode::INTERNAL_SERVER_ERROR;
                    resp.headers_mut().insert(
                        axum::http::header::CONTENT_TYPE,
                        axum::http::HeaderValue::from_static("text/plain"),
                    );
                    *resp.body_mut() = err.to_string().into();
                }
            }
        }
    }

    pub fn refresh(&self, parts: &Parts, aud: &[&str]) -> Result<TokenPair, AuthError> {
        // generate refresh token and bind it to cookie just like login
        let user = self.extract_user(parts, aud, true)?;
        let pair = self.create_token_pair(user, aud)?;
        return Ok(pair)
    }

    pub fn logout(&self, refresh: bool, resp: &mut Response) {
        let cookie_conf = if refresh {
            &self.refresh_cookie_conf
        } else {
            &self.access_cookie_conf
        };
        if let Some(conf) = cookie_conf.as_ref() {
            let c = Cookie::build((conf.name.as_str(), ""))
                .path(conf.path.as_str())
                .max_age(time::Duration::seconds(0))
                .build();
            match c.to_string().parse() {
                Ok(hv) => {
                    resp.headers_mut().append(header::SET_COOKIE, hv);
                }
                Err(err) => {
                    *resp.status_mut() = axum::http::StatusCode::INTERNAL_SERVER_ERROR;
                    resp.headers_mut().insert(
                        axum::http::header::CONTENT_TYPE,
                        axum::http::HeaderValue::from_static("text/plain"),
                    );
                    *resp.body_mut() = err.to_string().into();
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    #[serde(rename = "user_id")]
    pub id: i64,
    pub kind: i64,
    pub is_staff: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JWTClaim {
    #[serde(default)]
    kid: String,
    #[serde(default)]
    jti: String,
    #[serde(default)]
    sub: String,
    #[serde(default)]
    aud: Vec<String>,
    iat: i64,
    exp: i64,
    #[serde(default)]
    refresh: bool,
    #[serde(flatten)]
    user: AuthUser
}

impl JWTClaim {
    pub fn new(user: &AuthUser, kid: &str, aud: Vec<String>, ttl: i64) -> Self {
        let now = unix_timestamp();
        Self {
            kid: kid.to_string(),
            jti: uuid::Uuid::new_v4().to_string(),
            sub: user.id.to_string(),
            aud,
            iat: now,
            exp: now + ttl,
            refresh: false,
            user: user.clone(),
        }
    }

    fn into_auth_user(self) -> AuthUser {
        self.user
    }

}



#[derive(Debug)]
pub enum AuthError {
    InvalidToken,
    MissingToken,
    ExpiredToken,
    InvalidSignature,
    Forbidden,
    InternalError(String),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid token"),
            AuthError::MissingToken => (StatusCode::UNAUTHORIZED, "Missing token"),
            AuthError::ExpiredToken => (StatusCode::UNAUTHORIZED, "Expired token"),
            AuthError::InvalidSignature => (StatusCode::UNAUTHORIZED, "Invalid signature"),
            AuthError::Forbidden => (StatusCode::FORBIDDEN, "Permission denied"),
            AuthError::InternalError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal error"),
        };

        (status, msg).into_response()
    }
}

//convert jsonwebtoken::errors::Error to JWTError
impl From<&jsonwebtoken::errors::Error> for AuthError {
    fn from(err: &jsonwebtoken::errors::Error) -> Self {
        match err.kind() {
            jsonwebtoken::errors::ErrorKind::InvalidToken => AuthError::InvalidToken,
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::ExpiredToken,
            jsonwebtoken::errors::ErrorKind::InvalidSignature => AuthError::InvalidSignature,
            _ => AuthError::InternalError(err.to_string()),
        }
    }
}

impl axum::extract::FromRequestParts<Site> for AuthUser {
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, site: &Site) -> Result<Self, Self::Rejection> {
        let refresh = false;
        let auth = site.authenticator();
        let user_result = auth.extract_user(parts, &[], refresh);
        let user = user_result?;
        Ok(user)
    }
}

pub trait AuthBackend{
    type Input: DeserializeOwned;

    fn name(&self) -> &'static str;

    async fn authenticate(&self, input: Self::Input) -> Result<AuthUser, AuthError>;

}

