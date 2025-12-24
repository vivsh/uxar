use std::collections::HashMap;

use crate::views::Router;
use super::embed::Dir;
use serde::{de::DeserializeOwned, Deserialize};

use crate::{site::Service, Site};



/// A very naive attempt to develop a django application-like structure in Rust. 
/// This application will be created only one time and will be used to handle all the requests.
/// All router, templates, static files, and migrations will be consumed one-time by the parent site.
/// Application conf will be stored in the site and can be accessed by handlers using extensions.
/// This is a trait that defines the basic structure of an application.
/// It provides methods to get the router, templates and static files.
/// from the site configuration toml file.
pub trait Application: Send + 'static{
    
    fn router(&self)->Router;

    fn templates_dir(&self)->Option<Dir>{
        None
    }

    fn static_dir(&self)->Option<Dir>{
        None
    }

    fn start(&mut self, site: Site){}

    fn stop(&mut self, site: Site){}
    
}