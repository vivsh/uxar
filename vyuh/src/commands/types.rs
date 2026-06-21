use std::{any::TypeId, sync::Arc};

use crate::{
    Site,
    callables::{self, Callable},
};

use super::args::{self, CommandArg};
use super::error::CommandError;

// ── public config ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct CommandConf {
    pub name: Option<String>,
    pub description: Option<String>,
}

impl CommandConf {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            description: None,
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

pub struct CommandArgs<T: callables::Payloadable> {
    inner: Arc<T>,
}

impl<T: callables::Payloadable> CommandArgs<T> {
    pub fn into_inner(self) -> Arc<T> {
        self.inner
    }
}

impl<T: callables::Payloadable> std::ops::Deref for CommandArgs<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: callables::Payloadable> AsRef<T> for CommandArgs<T> {
    fn as_ref(&self) -> &T {
        &self.inner
    }
}

impl<T: callables::Payloadable> From<Arc<T>> for CommandArgs<T> {
    fn from(value: Arc<T>) -> Self {
        Self { inner: value }
    }
}

impl<C, T> callables::FromContext<C> for CommandArgs<T>
where
    C: callables::IntoPayloadData + Send + 'static,
    T: callables::Payloadable,
{
    fn from_context(ctx: C) -> Result<Self, callables::CallError> {
        let payload_data = ctx.into_payload_data();
        let value = payload_data
            .downcast_arc::<T>()
            .ok_or(callables::CallError::TypeMismatch)?;
        Ok(Self::from(value))
    }
}

impl<T: callables::Payloadable> callables::IntoArgPart for CommandArgs<T> {
    fn into_arg_part() -> callables::ArgPart {
        callables::ArgPart::Body(
            callables::TypeSchema::wrap::<T>(),
            "application/json".into(),
        )
    }
}

// ── handler alias ─────────────────────────────────────────────────────────────

pub type CommandHandlerIn = Callable<CommandContext, CommandError>;

// ── context ───────────────────────────────────────────────────────────────────

pub struct CommandContext {
    pub(super) site: Site,
    pub(super) payload: callables::PayloadData,
}

impl CommandContext {
    pub(super) fn new(site: Site, payload: callables::PayloadData) -> Self {
        Self { site, payload }
    }
}

impl callables::IntoPayloadData for CommandContext {
    fn into_payload_data(self) -> callables::PayloadData {
        self.payload
    }
}

impl callables::HasSite for CommandContext {
    fn site(&self) -> &Site {
        &self.site
    }
}

// ── command ───────────────────────────────────────────────────────────────────

pub(crate) struct Command {
    pub(crate) handler: CommandHandlerIn,
    pub(crate) options: CommandConf,
    pub(crate) args: Vec<CommandArg>,
    pub(crate) parser:
        fn(&str, &[&str], &[CommandArg]) -> Result<callables::PayloadData, CommandError>,
}

impl Command {
    pub(crate) fn operation(&self) -> callables::Operation {
        let spec = self.handler.inspect();
        callables::Operation::from_specs(callables::OperationKind::Command, spec)
            .with_conf(&self.options)
    }
}

// ── factory ───────────────────────────────────────────────────────────────────

pub(crate) fn command<T, H, Args>(handler: H, options: CommandConf) -> Result<Command, CommandError>
where
    T: callables::Payloadable,
    H: callables::Specable<Args, Output = Result<(), CommandError>> + Send + Sync + 'static,
    Args: callables::FromContext<CommandContext>
        + callables::IntoArgSpecs
        + callables::HasPayload<T>
        + Send
        + 'static,
{
    let mut callable: Callable<CommandContext, CommandError> = Callable::new(handler);
    let mut settings = schemars::generate::SchemaSettings::default();
    settings.inline_subschemas = true;
    let mut generator = schemars::SchemaGenerator::new(settings);
    let schema = generator.subschema_for::<T>();
    let parsed_args = args::parse_schema_to_args(&schema)?;
    let parser: fn(&str, &[&str], &[CommandArg]) -> Result<callables::PayloadData, CommandError> =
        |command_name, cli, arg_defs| {
            let obj: T = args::parse_args(command_name, cli, arg_defs)?;
            Ok(callables::PayloadData::new(obj))
        };
    callable.type_id = TypeId::of::<T>();
    Ok(Command {
        handler: callable,
        options,
        args: parsed_args,
        parser,
    })
}
