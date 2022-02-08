use vrl_core::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct GetMetadataField;

impl Function for GetMetadataField {
    fn identifier(&self) -> &'static str {
        "get_metadata_field"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "key",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "Get the datadog api key",
            source: r#"get_metadata_field("datadog_api_key")"#,
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

        Ok(Box::new(GetMetadataFieldFn { key }))
    }
}

#[derive(Debug, Clone)]
struct GetMetadataFieldFn {
    key: String,
}

impl Expression for GetMetadataFieldFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        ctx.target()
            .get_metadata(&self.key)
            .map(|value| value.unwrap_or(Value::Null))
            .map_err(Into::into)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().bytes().add_null()
    }
}
