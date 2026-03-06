
mod specs;
mod patch;
mod callables;
mod extractors;
mod operations;

#[cfg(test)]
mod tests;

// Re-export all public types and traits
pub use specs::{
    // Core traits
    Payloadable,
    Specable,
    IntoArgPart,
    IntoReturnPart,
    IntoArgSpecs,
    IntoLayerParts,
    IntoHandlerSpec,
    HasPayload,
    
    // Error type
    CallError,
    
    // Main types
    CallSpec,
    // Spec types
    ArgSpec,
    LayerSpec,
    ArgPart,
    ReturnSpec,
    ReturnPart,
    ReceiverSpec,
    TypeSchema,
};

pub use callables::{
    // Extraction traits
    FromContext,
    FromContextParts,
    IntoPayloadData,
    IntoOutput,
    
    // Runtime types
    Callable,
    PayloadData,
};

pub use patch::{
    ArgPatch,
    PatchOp,
    ReturnPatch,
};

pub use extractors::{
    HasSite,
    FromSite,
    Payload,
};

pub use operations::{
    Operation,
    OperationKind,
};
