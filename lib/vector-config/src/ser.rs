use serde_json::Value;
use std::{cell::RefCell, marker::PhantomData};

use serde::{Serialize, Serializer};

use crate::{
    schema::{SchemaGenerator, SchemaObject},
    Configurable, GenerateError, Metadata, ToValue,
};

/// Delegated serialization.
///
/// This adapter type lets us delegate the work of serializing an `I` by delegating it to `H`, where
/// `H` represents some sort of helper type (a la `serde_with`) that takes `I` and serializes it in
/// a specific way. This is a common pattern for using standard Rust types on the interior (such as
/// `std::time::Duration`) but (de)serializing them in more friendly/ergonomic forms, like `10s`,
/// which requires the use of custom (de)serialize functions or types.
///
/// Concretely, in the codegen, if a value has no mitigating configuration, we treat it as if it is
/// already `Serialize`, and use it directly. Otherwise, if specific attributes are in place that
/// indicate the use of an optional helper type, we construct `Delegated<I, H>` where `H` is the
/// value passed to `#[serde(with = "...")]`.
///
/// Astute readers may realize: "but isn't the value of `with` supposed to be a module path where a
/// custom `serialize` and/or `deserialize` function exist?", and they would be correct insofar as
/// that is what the `serde` documentation states. However, that value is used to construct a _path_
/// to a function to be called, which means that if you simply specify a type that is in scope, and
/// it has a public function called `serialize` and/or `deserialize`, you end up with a path like
/// `MyCustomType::serialize`, which is valid, and `serde` and `serde_with` use this fact to support
/// generating custom types that can be used for (de)serialization.
///
/// This means we do some extra work up front to avoid misclassifying usages of `#[serde(with =
/// "...")]` that are using module paths, and as such, this also means that those usages are not
/// supported.
pub struct Delegated<I, H> {
    input: I,
    _helper: PhantomData<fn(H)>,
}

// Adapter implementation for `serde_with` where the helper `H` is able to serialize `I`.
impl<I, H> Serialize for Delegated<I, serde_with::As<H>>
where
    H: serde_with::SerializeAs<I>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        H::serialize_as(&self.input, serializer)
    }
}

impl<I, H> From<I> for Delegated<I, serde_with::As<H>>
where
    H: serde_with::SerializeAs<I>,
{
    fn from(input: I) -> Self {
        Self {
            input,
            _helper: PhantomData,
        }
    }
}

// Passthrough implementation for any `H` which is `Configurable`.
impl<I, H> Configurable for Delegated<I, H>
where
    H: Configurable,
{
    fn referenceable_name() -> Option<&'static str> {
        // Forward to the underlying `H`.
        H::referenceable_name()
    }

    fn metadata() -> Metadata {
        // Forward to the underlying `H`.
        //
        // We have to convert from `Metadata` to `Metadata` which erases the default value,
        // notably, but delegated helpers should never actually have default values, so this is
        // essentially a no-op.
        H::metadata().convert()
    }

    fn validate_metadata(metadata: &Metadata) -> Result<(), GenerateError> {
        // Forward to the underlying `H`.
        //
        // We have to convert from `Metadata` to `Metadata` which erases the default value,
        // notably, but `serde_with` helpers should never actually have default values, so this is
        // essentially a no-op.
        let converted = metadata.convert();
        H::validate_metadata(&converted)
    }

    fn generate_schema(gen: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        // Forward to the underlying `H`.
        H::generate_schema(gen)
    }
}

impl<I, H> ToValue for Delegated<I, H>
where
    H: Configurable,
    Delegated<I, H>: Serialize,
{
    fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}
