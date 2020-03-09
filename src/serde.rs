pub fn default_true() -> bool {
    true
}

pub fn default_false() -> bool {
    false
}

pub fn to_string(value: impl serde::Serialize) -> String {
    let value = serde_json::to_value(value).unwrap();
    value.as_str().unwrap().into()
}
