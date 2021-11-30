use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct SetEventMetadata;

impl Function for SetEventMetadata {
    fn identifier(&self) -> &'static str {
        "set_event_metadata"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "key",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "Sets the datadog api key",
            source: r#"set_event_metadata("datadog_api_key", "abc123")"#,
            result: Ok("null"),
        }]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let keys = vec![value!("datadog_api_key")];
        let key = arguments
            .required_enum("key", &keys)?
            .try_bytes_utf8_lossy()
            .expect("key not bytes")
            .to_string();
        let value = arguments.required("value");

        Ok(Box::new(SetEventMetadataFn { key, value }))
    }
}

#[derive(Debug, Clone)]
struct SetEventMetadataFn {
    key: String,
    value: Box<dyn Expression>,
}

impl Expression for SetEventMetadataFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?.try_bytes_utf8_lossy()?.to_string();
        ctx.target_mut().set_metadata(&self.key, value)?;
        Ok(Value::Null)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().null()
    }
}
