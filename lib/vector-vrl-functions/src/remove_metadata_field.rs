use vrl_core::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct RemoveMetadataField;

impl Function for RemoveMetadataField {
    fn identifier(&self) -> &'static str {
        "remove_metadata_field"
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
            title: "Removes the datadog api key",
            source: r#"remove_metadata_field("datadog_api_key")"#,
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

        Ok(Box::new(RemoveMetadataFieldFn { key }))
    }
}

#[derive(Debug, Clone)]
struct RemoveMetadataFieldFn {
    key: String,
}

impl Expression for RemoveMetadataFieldFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        ctx.target_mut().remove_metadata(&self.key)?;
        Ok(Value::Null)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().null()
    }
}
