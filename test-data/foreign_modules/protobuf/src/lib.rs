use prost::Message;
use foreign_modules::guest::{get, insert};

pub mod items {
    include!(concat!(env!("OUT_DIR"), "/messages.rs"));
}

#[no_mangle]
pub extern "C" fn process() -> bool {
    let result = get("test");
    match result.unwrap() {
        Some(value) => {
            let value_str = value.as_str().expect("Protobuf field not a str");
            let decoded = crate::items::AddressBook::decode(
                value_str.as_bytes(),
            )
            .unwrap();
            let reencoded = serde_json::to_string(&decoded).unwrap();
            insert("processed", reencoded).unwrap();
            true
        }
        None => {
            false
        }
    };
    let result = get("processed");
    true
}
