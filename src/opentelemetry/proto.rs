#![allow(warnings)]

pub mod collector {
    pub mod logs {
        pub mod v1 {
            tonic::include_proto!("opentelemetry.proto.collector.logs.v1");
        }
    }
}

pub mod common {
    pub mod v1 {
        tonic::include_proto!("opentelemetry.proto.common.v1");
    }
}

pub mod logs {
    pub mod v1 {
        tonic::include_proto!("opentelemetry.proto.logs.v1");
    }
}

pub mod resource {
    pub mod v1 {
        tonic::include_proto!("opentelemetry.proto.resource.v1");
    }
}
