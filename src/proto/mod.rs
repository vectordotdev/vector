#[cfg(any(feature = "sources-vector", feature = "sinks-vector"))]
use crate::event::proto as event;

#[cfg(any(feature = "sources-vector", feature = "sinks-vector"))]
pub mod vector;

#[cfg(feature = "sinks-datadog_metrics")]
pub mod fds {
    use once_cell::sync::OnceCell;
    use prost_reflect::DescriptorPool;

    static PROTOBUF_FDS: OnceCell<DescriptorPool> = OnceCell::new();

    pub fn get_protobuf_descriptors() -> &'static DescriptorPool {
        PROTOBUF_FDS.get_or_init(|| {
            DescriptorPool::decode(include_bytes!(concat!(env!("OUT_DIR"), "/protobuf-fds.bin")).as_ref())
                .expect("should not fail to decode protobuf file descriptor set generated from build script")
        })
    }
}
