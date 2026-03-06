

use crate::{bundles::Bundle, embed};



#[derive(Debug, thiserror::Error)]
pub enum TemplateError{

    #[error("Path error: {0}")]
    PathError(String),

    #[error("Template file error: {0}")]
    FileError(String),

    #[error("Template rendering error: {0}")]
    ParseError(String),

    #[error("Template not found: {0}")]
    NotFound(String),

    #[error("Render error: {0}")]
    RenderError(#[from] minijinja::Error),
}


pub struct TemplateEngine{
    env: minijinja::Environment<'static>,
}

impl TemplateEngine {

    pub fn new() -> Self {
        let env = minijinja::Environment::new();
        TemplateEngine { env }
    }

    pub fn render<S: serde::Serialize>(&self, template_name: &str, context: &S) -> Result<String, TemplateError> {
        self.env
            .get_template(template_name)
            .map_err(|e| TemplateError::NotFound(format!("Template '{}' not found: {}", template_name, e)))?
            .render(context)
            .map_err(|e| TemplateError::RenderError(e).into())
    }

    pub(crate) fn inject_templates(
        &mut self,
        template_dir: Option<&str>,
        bundle: &Bundle,
    ) -> Result<(), TemplateError> {
        let mut dir_vec: Vec<embed::Dir> = Vec::new();

        // iterate over files in conf.templates_dir
        if let Some(templates_dir) = &template_dir {
            let path = crate::conf::project_dir().join(templates_dir);
            let dir = rust_silos::Silo::new(path.to_str().unwrap_or(""));
            dir_vec.push(dir.into());
        }

        for asset_dir in &bundle.asset_dirs {
            dir_vec.push(asset_dir.clone());
        }

        for file in embed::DirSet::new(dir_vec).walk() {
            let prefix = "templates/";
            let path = file.path();

            if ! path.starts_with(prefix){
                continue;
            }

            let name = match path.strip_prefix(prefix).map(|s|s.to_string_lossy().to_string()){
                Ok(s)=>s,
                Err(_) => return Err(TemplateError::PathError(format!("Failed to strip prefix from template path: {}", path.display()))),
            };
            
            let content = file.read_bytes_sync().map_err(|e| {
                TemplateError::FileError(format!(
                    "Failed to read template file: {}: {}",
                    path.display(),
                    e
                ))
            })?;
            let body = String::from_utf8(content).map_err(|e| {
                TemplateError::FileError(format!(
                    "Invalid UTF-8 in template file: {}: {}",
                    path.display(),
                    e
                ))
            })?;
            if let Err(err) = self.env.add_template_owned(name, body) {
                return Err(TemplateError::RenderError(err).into());
            }
        }

        Ok(())
    }

    pub(crate) fn manager<'a>(&'a self) -> TemplateManager<'a> {
        TemplateManager { engine: self }
    }


}


pub struct TemplateManager<'a>{
    engine: &'a TemplateEngine,
}

impl<'a> TemplateManager<'a> {
    
    pub fn render<S: serde::Serialize>(&self, template_name: &str, context: &S) -> Result<String, TemplateError> {
        self.engine.render(template_name, context)
    }

}