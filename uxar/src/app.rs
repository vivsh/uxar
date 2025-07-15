use std::collections::HashMap;

use axum::Router;
use super::embed::Dir;
use serde::{de::DeserializeOwned, Deserialize};

use crate::{site::Service, Site};



/// A very naive attempt to develop a django application-like structure in Rust. 
/// This application will be created only one time and will be used to handle all the requests.
/// All router, templates, static files, and migrations will be consumed one-time by the parent site.
/// Application conf will be stored in the site and can be accessed by handlers using extensions.
/// This is a trait that defines the basic structure of an application.
/// It provides methods to get the router, templates, migrations, tagged-sql and static files.
/// from the site configuration toml file.
pub trait Application: Send + 'static{
    
    fn router(&self)->Router<Site>;

    fn templates_dir(&self)->Option<Dir>{
        None
    }

    fn static_dir(&self)->Option<Dir>{
        None
    }

    fn migration_dir(&self)->Option<Dir>{
        None    
    }

    fn sql_dir(&self)->Option<Dir>{
        None
    }

    fn services(&self) -> Vec<Box<dyn Service>> {
        Vec::new()
    }

    fn start(&mut self, site: Site){}

    fn stop(&mut self, site: Site){}
    
}


#[derive(Clone, Debug)]
pub struct RouterApplication( pub Router<Site>);


impl Application for RouterApplication {
    fn router(&self) -> Router<Site> {
        self.0.clone()
    }
}


impl From<Router<Site>> for RouterApplication {
    fn from(router: Router<Site>) -> Self {
        RouterApplication(router)
    }
}

pub trait IntoApplication {
    fn into_app(self) -> impl Application + Send + Sync + 'static;
}

impl IntoApplication for Router<Site> {
    fn into_app(self) -> impl Application + Send + Sync + 'static {
        RouterApplication(self)
    }
}

impl<A: Application + Send + Sync + 'static> IntoApplication for A {
    fn into_app(self) -> impl Application + Send + Sync + 'static {
        self
    }
}

