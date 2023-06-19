#[cfg(feature = "regex")]
use once_cell::sync::Lazy;
#[cfg(feature = "regex")]
use regex::Regex;

// Environment variable names can have any characters from the Portable Character Set other
// than NUL.  However, for Vector's interpolation, we are closer to what a shell supports which
// is solely of uppercase letters, digits, and the '_' (that is, the `[:word:]` regex class).
// In addition to these characters, we allow `.` as this commonly appears in environment
// variable names when they come from a Java properties file.
//
// https://pubs.opengroup.org/onlinepubs/000095399/basedefs/xbd_chap08.html
pub const RAW_REGEX: &str = r"(?x)
\$\$|
\$([[:word:].]+)|
\$\{([[:word:].]+)(?:(:?-|:?\?)([^}]*))?\}";

#[cfg(feature = "regex")]
pub static REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(RAW_REGEX).unwrap()
});
