pub mod event {
    include!(concat!(env!("OUT_DIR"), "/event.rs"));
}

pub mod grpc {
    include!(concat!(env!("OUT_DIR"), "/vector.rs"));
}
