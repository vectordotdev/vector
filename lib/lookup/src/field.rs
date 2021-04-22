use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref VALID_FIELD: Regex = Regex::new("^[0-9]*[a-zA-Z_][0-9a-zA-Z_]*$").unwrap();
}

pub(crate) fn is_valid_fieldname(name: &str) -> bool {
    VALID_FIELD.is_match(&name)
}
