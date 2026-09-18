use std::cell::RefCell;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use vector_config::{
    Configurable, GenerateError, Metadata, ToValue,
    schema::{SchemaGenerator, SchemaObject},
};

/// Portable configuration whose runtime requires Unix.
///
/// Serialization and schema generation are identical to those of `T`. Runtime
/// construction goes through [`Self::on_unix`]; platform-independent
/// inspection explicitly uses [`Self::platform_independent`] instead.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct UnixOnly<T>(T);

impl<T> From<T> for UnixOnly<T> {
    fn from(config: T) -> Self {
        Self(config)
    }
}

impl<T> UnixOnly<T> {
    /// Borrows the configuration without checking platform support.
    ///
    /// This escape hatch is for configuration inspection, not runtime construction.
    /// It deliberately does not provide implicit access through `Deref`.
    pub const fn platform_independent(&self) -> &T {
        &self.0
    }

    /// Borrows the configuration while retaining the runtime platform check.
    pub const fn as_ref(&self) -> UnixOnly<&T> {
        UnixOnly(&self.0)
    }

    /// Constructs the runtime on Unix, or returns an unsupported-platform error.
    ///
    /// Pass the closure with `#[cfg(unix)]` so Unix-only APIs in its body are not
    /// compiled on other targets. Context is accepted on every target and is
    /// dropped, together with the configuration, if Unix is unavailable.
    pub fn on_unix<C, R>(
        self,
        context: C,
        #[cfg(unix)] operation: impl FnOnce(T, C) -> crate::Result<R>,
    ) -> crate::Result<R> {
        #[cfg(unix)]
        {
            operation(self.0, context)
        }
        #[cfg(not(unix))]
        {
            let _ = (self, context);
            Err(format!(
                "Unix-only configuration is not supported on {}.",
                std::env::consts::OS
            )
            .into())
        }
    }
}

impl<T: Configurable> Configurable for UnixOnly<T> {
    fn referenceable_name() -> Option<&'static str> {
        T::referenceable_name()
    }

    fn is_optional() -> bool {
        T::is_optional()
    }

    fn metadata() -> Metadata {
        T::metadata()
    }

    fn validate_metadata(metadata: &Metadata) -> Result<(), GenerateError> {
        T::validate_metadata(metadata)
    }

    fn generate_schema(
        generator: &RefCell<SchemaGenerator>,
    ) -> Result<SchemaObject, GenerateError> {
        T::generate_schema(generator)
    }
}

impl<T: ToValue> ToValue for UnixOnly<T> {
    fn to_value(&self) -> Value {
        self.0.to_value()
    }
}
