use prost::Message;
use vector_wasm::{hostcall, Registration};

// Import from prost.
pub mod items {
    include!(concat!(env!("OUT_DIR"), "/messages.rs"));
}

#[no_mangle]
pub extern "C" fn init() -> *mut Registration {
    &mut Registration::transform().set_wasi(true) as *mut Registration
}

#[no_mangle]
pub extern "C" fn process() -> usize {
    let result = hostcall::get("test");
    match result.unwrap() {
        Some(value) => {
            let value_str = value.as_str().expect("Protobuf field not a str");
            let decoded = crate::items::AddressBook::decode(value_str.as_bytes()).unwrap();
            let recoded = serde_json::to_string(&decoded).unwrap();
            hostcall::insert("processed", recoded).unwrap();
            true
        }
        None => false,
    };
    Default::default()
}

#[no_mangle]
pub extern "C" fn shutdown() {
    ();
}
