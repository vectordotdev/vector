use remap::prelude::*;
use std::cell::RefCell;
use std::time::{Duration, Instant};

thread_local! {
    static HOSTNAME: RefCell<(Instant, Result<Value>)> = RefCell::new((Instant::now(), get_hostname_inner()));
}

fn get_hostname_inner() -> Result<Value> {
    Ok(hostname::get()
        .map_err(|error| format!("failed to get hostname: {}", error))?
        .into_string()
        .map_err(|error| format!("failed to convert hostname to string: {:?}", error))?
        .into())
}

#[derive(Clone, Copy, Debug)]
pub struct GetHostname;

impl Function for GetHostname {
    fn identifier(&self) -> &'static str {
        "get_hostname"
    }

    fn compile(&self, _: ArgumentList) -> Result<Box<dyn Expression>> {
        Ok(Box::new(GetHostnameFn))
    }
}

#[derive(Debug, Clone)]
struct GetHostnameFn;

impl Expression for GetHostnameFn {
    fn execute(&self, _: &mut state::Program, _: &mut dyn Object) -> Result<Value> {
        HOSTNAME.with(|pair| {
            let mut pair = pair.borrow_mut();
            let now = Instant::now();
            if pair.0 < now {
                pair.0 = now + Duration::from_millis(10);
                pair.1 = get_hostname_inner();
            }

            pair.1.clone()
        })
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef {
            kind: value::Kind::Bytes,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    remap::test_type_def![static_def {
        expr: |_| GetHostnameFn,
        def: TypeDef {
            kind: value::Kind::Bytes,
            ..Default::default()
        },
    }];

    #[test]
    fn get_hostname() {
        let mut state = state::Program::default();
        let mut object: Value = map![].into();
        let value = GetHostnameFn.execute(&mut state, &mut object).unwrap();

        assert!(matches!(&value, Value::Bytes(_)));
    }
}
