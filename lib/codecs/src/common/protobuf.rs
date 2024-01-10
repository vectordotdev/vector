use prost_reflect::{DescriptorPool, MessageDescriptor};
use std::path::Path;

/// Load a `MessageDescriptor` from a specific message type from the given descriptor set file.
///
/// The path should point to the output of `protoc -o <path> ...`
pub fn get_message_descriptor(
    descriptor_set_path: &Path,
    message_type: &str,
) -> vector_common::Result<MessageDescriptor> {
    let b = std::fs::read(descriptor_set_path).map_err(|e| {
        format!("Failed to open protobuf desc file '{descriptor_set_path:?}': {e}",)
    })?;
    let pool = DescriptorPool::decode(b.as_slice()).map_err(|e| {
        format!("Failed to parse protobuf desc file '{descriptor_set_path:?}': {e}")
    })?;
    pool.get_message_by_name(message_type).ok_or_else(|| {
        format!("The message type '{message_type}' could not be found in '{descriptor_set_path:?}'")
            .into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_get_message_descriptor() {
        let path = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("tests/data/protobuf/protos/test.desc");
        let message_descriptor = get_message_descriptor(&path, "test.Integers").unwrap();
        assert_eq!("Integers", message_descriptor.name());
        assert_eq!(4, message_descriptor.fields().count());
    }
}
