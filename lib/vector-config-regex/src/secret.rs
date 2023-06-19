#[cfg(feature = "regex")]
use once_cell::sync::Lazy;
#[cfg(feature = "regex")]
use regex::Regex;

// The following regex aims to extract a pair of strings, the first being the secret backend name
// and the second being the secret key. Here are some matching & non-matching examples:
// - "SECRET[backend.secret_name]" will match and capture "backend" and "secret_name"
// - "SECRET[backend.secret.name]" will match and capture "backend" and "secret.name"
// - "SECRET[backend..secret.name]" will match and capture "backend" and ".secret.name"
// - "SECRET[secret_name]" will not match
// - "SECRET[.secret.name]" will not match
pub const RAW_REGEX: &str = r"SECRET\[([[:word:]]+)\.([[:word:].]+)\]";

#[cfg(feature = "regex")]
pub static REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(RAW_REGEX).unwrap());
