pub mod convert;

/// Service stub and clients.
pub mod envoy {
    pub mod service {
        pub mod accesslog {
            pub mod v3 {
                include!(concat!(env!("OUT_DIR"), "/envoy.service.accesslog.v3.rs"));
            }
        }
    }

    pub mod config {
        pub mod core {
            pub mod v3 {
                include!(concat!(env!("OUT_DIR"), "/envoy.config.core.v3.rs"));
            }
        }
    }

    pub mod data {
        pub mod accesslog {
            pub mod v3 {
                include!(concat!(env!("OUT_DIR"), "/envoy.data.accesslog.v3.rs"));
            }
        }
    }

    pub mod r#type {
        pub mod v3 {
            include!(concat!(env!("OUT_DIR"), "/envoy.r#type.v3.rs"));
        }
    }
}

pub mod xds {
    pub mod core {
        pub mod v3 {
            include!(concat!(env!("OUT_DIR"), "/xds.core.v3.rs"));
        }
    }
}
