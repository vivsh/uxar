use crate::Site;

use super::error::CommandError;
use super::registry::CommandRegistry;

/// Thin dispatch wrapper around [`CommandRegistry`].
pub struct CommandEngine {
    registry: CommandRegistry,
}

impl CommandEngine {
    pub fn new(registry: CommandRegistry) -> Self {
        Self { registry }
    }

    pub async fn execute(
        &self,
        name: &str,
        args: &[&str],
        site: Site,
    ) -> Result<(), CommandError> {
        self.registry.execute(name, args, site).await
    }

    pub fn help(&self) -> String {
        self.registry.execute_help()
    }
}
