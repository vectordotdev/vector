#![macro_use]

macro_rules! required {
    ($state:expr, $object:expr, $fn:expr, $($pattern:pat $(if $if:expr)? => $then:expr),+ $(,)?) => {
        match $fn.execute($state, $object)? {
            Some(value) => match value {
                $($pattern $(if $if)? => $then,)+
                v => panic!(v),
            }
            None => return Ok(None)
        }
    }
}

macro_rules! optional {
    ($state:expr, $object:expr, $fn:expr, $($pattern:pat $(if $if:expr)? => $then:expr),+ $(,)?) => {
        $fn.as_ref()
            .map(|v| v.execute($state, $object))
            .transpose()?
            .map(|v| match v {
                Some(value) => match value {
                    $($pattern $(if $if)? => Some($then),)+
                    v => panic!(v),
                }
                None => None,
            })
            .flatten()
    }
}

mod contains;
mod del;
mod downcase;
mod format_timestamp;
mod md5;
mod now;
mod only_fields;
mod sha1;
mod split;
mod to_bool;
mod to_float;
mod to_int;
mod to_string;
mod to_timestamp;
mod upcase;
mod uuid_v4;

pub use self::md5::Md5;
pub use self::sha1::Sha1;
pub use contains::Contains;
pub use del::Del;
pub use downcase::Downcase;
pub use format_timestamp::FormatTimestamp;
pub use now::Now;
pub use only_fields::OnlyFields;
pub use split::Split;
pub use to_bool::ToBool;
pub use to_float::ToFloat;
pub use to_int::ToInt;
pub use to_string::ToString;
pub use to_timestamp::ToTimestamp;
pub use upcase::Upcase;
pub use uuid_v4::UuidV4;

use remap::{Result, Value};

fn convert_value_or_default(
    value: Result<Option<Value>>,
    default: Option<Result<Option<Value>>>,
    convert: fn(Value) -> Result<Value>,
) -> Result<Option<Value>> {
    value
        .and_then(|opt| opt.map(convert).transpose())
        .or_else(|err| {
            default
                .ok_or(err)?
                .and_then(|opt| opt.map(convert).transpose())
        })
}

fn is_scalar_value(value: &Value) -> bool {
    use Value::*;

    match value {
        Integer(_) | Float(_) | String(_) | Boolean(_) => true,
        Timestamp(_) | Map(_) | Array(_) | Null => false,
    }
}
