

use serde::{Deserialize, Serialize};
use thiserror::Error;
use crate::{app::Application, auth::AuthConf};



#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SiteConf{
    
    pub host: String,
    
    pub port: u16,    

    pub project_dir: String,

    pub database: String,

    pub secret_key: String,
    
    pub static_dir: Option<String>,

    pub static_url: Option<String>,

    pub media_url: Option<String>,

    pub media_dir: Option<String>,
    
    pub templates_dir: Option<String>,

    pub auth: AuthConf

}


impl Default for SiteConf {

    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 8080,
            project_dir: ".".to_string(),
            database: "".to_string(),
            secret_key: "".to_string(),
            static_dir: None,
            static_url: None,
            media_url: None,
            media_dir: None,
            templates_dir: None,
            auth: AuthConf::default(),
        }

    }

}