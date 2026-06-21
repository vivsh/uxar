mod callables;
mod extractors;
mod operations;
mod patch;
pub(crate) mod specs;

#[cfg(test)]
mod tests;

// Re-export all public types and traits
pub use specs::{
    ArgPart,
    // Spec types
    ArgSpec,
    // Error type
    CallError,

    // Main types
    CallSpec,
    HasPayload,

    IntoArgPart,
    IntoArgSpecs,
    IntoHandlerSpec,
    IntoLayerParts,
    IntoReturnPart,
    LayerSpec,
    // Core traits
    Payloadable,
    ReceiverSpec,
    ReturnPart,
    ReturnSpec,
    Specable,
    TypeSchema,
};

pub use callables::{
    // Runtime types
    Callable,
    // Extraction traits
    FromContext,
    FromContextParts,
    IntoOutput,

    IntoPayloadData,
    PayloadData,
};

pub use patch::{ArgPatch, PatchOp, ReturnPatch};

pub use extractors::{FromSite, HasSite, Payload};

pub use operations::{Operation, OperationKind};
