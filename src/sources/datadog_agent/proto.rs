mod include {
    include!(concat!(env!("OUT_DIR"), "/dd-agent-protos/mod.rs"));
}

pub mod metrics {
    pub use super::include::dd_metric::*;
    pub use super::include::ddsketch_full::*;
}

pub mod traces {
    pub use super::include::dd_trace::*;
}
