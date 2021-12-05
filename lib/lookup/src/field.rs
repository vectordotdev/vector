use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref VALID_FIELD: Regex = Regex::new("^[0-9]*[a-zA-Z_][0-9a-zA-Z_]*$").unwrap();
}

/// A valid fieldname can contain alphanumeric characters and an underscore.
/// It may start with a number, but has to consist of more than just a number.
/// Fields that have other characters can be used, but need to be quoted.
pub(crate) fn is_valid_fieldname(name: &str) -> bool {
    VALID_FIELD.is_match(name)
}
