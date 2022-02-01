use bytes::Bytes;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct UuidV4;

impl Function for UuidV4 {
    fn identifier(&self) -> &'static str {
        "uuid_v4"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "generate UUID v4",
            source: r#"uuid_v4() != """#,
            result: Ok("true"),
        }]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        _: ArgumentList,
    ) -> Compiled {
        Ok(Box::new(UuidV4Fn))
    }
}

#[derive(Debug, Clone, Copy)]
struct UuidV4Fn;

impl Expression for UuidV4Fn {
    fn resolve(&self, _: &mut Context) -> Resolved {
        let mut buf = [0; 36];
        let uuid = uuid::Uuid::new_v4().to_hyphenated().encode_lower(&mut buf);

        Ok(Bytes::copy_from_slice(uuid.as_bytes()).into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().bytes()
    }
}

#[cfg(test)]
mod tests {
    use vector_common::TimeZone;

    use super::*;

    test_type_def![default {
        expr: |_| { UuidV4Fn },
        want: TypeDef::new().infallible().bytes(),
    }];

    #[test]
    fn uuid_v4() {
        let mut state = vrl::state::Runtime::default();
        let mut object: Value = map![].into();
        let tz = TimeZone::default();
        let mut ctx = Context::new(&mut object, &mut state, &tz);
        let value = UuidV4Fn.resolve(&mut ctx).unwrap();

        assert!(matches!(&value, Value::Bytes(_)));

        match value {
            Value::Bytes(val) => {
                let val = String::from_utf8_lossy(&val);
                uuid::Uuid::parse_str(&val).expect("valid UUID V4");
            }
            _ => unreachable!(),
        }
    }
}
