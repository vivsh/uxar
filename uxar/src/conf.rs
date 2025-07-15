use crate::{app::Application, auth::AuthConf};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticDir {
    pub path: String,
    pub url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SiteConf {
    pub host: String,

    pub port: u16,

    pub project_dir: String,

    pub database: String,

    pub secret_key: String,

    pub static_dirs: Vec<StaticDir>,

    pub upload_dir: Option<String>,

    pub templates_dir: Option<String>,

    pub auth: AuthConf,
}

impl Default for SiteConf {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 8080,
            project_dir: ".".to_string(),
            database: "".to_string(),
            secret_key: "".to_string(),
            static_dirs: vec![],
            upload_dir: None,
            templates_dir: None,
            auth: AuthConf::default(),
        }
    }
}

impl SiteConf {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();

        #[cfg(test)]
        {
            dotenvy::from_filename_override(".env.test").ok();
        }

        #[cfg(all(debug_assertions, not(test)))]
        {
            dotenvy::from_filename_override(".env.dev").ok();
        }

        #[cfg(not(any(debug_assertions, test)))]
        {
            dotenvy::from_filename_override(".env.prod").ok();
        }

        let database =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres:///uxar".to_string());
        let secret_key =
            std::env::var("SECRET_KEY").unwrap_or_else(|_| "default_secret_key".to_string());
        let host = std::env::var("HOST").unwrap_or_else(|_| "localhost".to_string());
        let port = std::env::var("PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse()
            .unwrap_or(8080);

        Self {
            host,
            port,
            project_dir: ".".to_string(),
            database,
            secret_key,
            ..Default::default()
        }
    }
}
