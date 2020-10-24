mod del;
mod only_fields;
mod split;
mod to_float;
mod to_int;
mod to_string;

pub use del::Del;
pub use only_fields::OnlyFields;
pub use split::Split;
pub use to_float::ToFloat;
pub use to_int::ToInt;
pub use to_string::ToString;

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
        Map(_) | Array(_) | Null => false,
    }
}
