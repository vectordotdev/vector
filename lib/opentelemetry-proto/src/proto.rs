/// Service stub and clients.
pub mod collector {
    pub mod trace {
        pub mod v1 {
            tonic::include_proto!("opentelemetry.proto.collector.trace.v1");
        }
    }
    pub mod logs {
        pub mod v1 {
            tonic::include_proto!("opentelemetry.proto.collector.logs.v1");
        }
    }
}

/// Common types used across all event types.
pub mod common {
    pub mod v1 {
        tonic::include_proto!("opentelemetry.proto.common.v1");
    }
}

/// Generated types used for logs.
pub mod logs {
    pub mod v1 {
        tonic::include_proto!("opentelemetry.proto.logs.v1");
    }
}

/// Generated types used for trace.
pub mod trace {
    pub mod v1 {
        tonic::include_proto!("opentelemetry.proto.trace.v1");
    }
}

/// Generated types used in resources.
pub mod resource {
    pub mod v1 {
        tonic::include_proto!("opentelemetry.proto.resource.v1");
    }
}
