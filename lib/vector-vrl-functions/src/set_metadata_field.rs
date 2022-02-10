use vrl_core::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct SetMetadataField;

impl Function for SetMetadataField {
    fn identifier(&self) -> &'static str {
        "set_metadata_field"
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
            source: r#"set_metadata_field("datadog_api_key", "abc123")"#,
            result: Ok("null"),
        }]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let keys = vec![value!("datadog_api_key"), value!("splunk_hec_token")];
        let key = arguments
            .required_enum("key", &keys)?
            .try_bytes_utf8_lossy()
            .expect("key not bytes")
            .to_string();
        let value = arguments.required("value");

        Ok(Box::new(SetMetadataFieldFn { key, value }))
    }
}

#[derive(Debug, Clone)]
struct SetMetadataFieldFn {
    key: String,
    value: Box<dyn Expression>,
}

impl Expression for SetMetadataFieldFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?.try_bytes_utf8_lossy()?.to_string();
        ctx.target_mut().set_metadata(&self.key, value)?;
        Ok(Value::Null)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().null()
    }
}
