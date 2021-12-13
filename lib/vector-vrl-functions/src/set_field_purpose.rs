use vector_core::schema;
use vrl_core::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct SetFieldPurpose;

impl Function for SetFieldPurpose {
    fn identifier(&self) -> &'static str {
        "set_field_purpose"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "target",
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "purpose",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "Sets custom field purpose",
            source: r#"set_field_purpose(.foo, "bar")"#,
            result: Ok("null"),
        }]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let query = arguments.required_query("target")?;

        let purpose = arguments
            .required_literal("purpose")?
            .to_value()
            .try_bytes_utf8_lossy()
            .expect("purpose not bytes")
            .into_owned();

        if !query.is_external() {
            return Err(Box::new(ExpressionError::from(format!(
                "purpose must be set on an external field: {}",
                query
            ))) as Box<dyn DiagnosticError>);
        }

        // Check for use of valid purposes.
        //
        // Note that purposes have a `Kind` attached to them, but we can't check those within this
        // context. This is because we have no knowledge of whether a follow-up component in Vector
        // will set the correct type for the field.
        //
        // Because of this, type checking happens at Vector's topology level _after_ the topology
        // has been built, and the final type of a field is known.
        match ctx.get_external_context_mut::<schema::TransformRegistry>() {
            Some(registry) if !registry.is_loading() && !registry.is_valid_purpose(&purpose) => {
                let message = "invalid purpose provided";
                let primary = format!(r#"the purpose "{}" is unused in this pipeline"#, purpose);
                let context = format!(
                    r#"must be one of: {}"#,
                    registry
                        .sink_purposes()
                        .into_iter()
                        .map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );

                let error =
                    CompilationError::new(message, vec![(primary, None)], vec![(context, None)]);

                return Err(Box::new(error));
            }
            Some(registry) => registry.register_purpose(query.path().clone(), &purpose),
            None => panic!("set_field_purpose requires external context"),
        };

        Ok(Box::new(SetFieldPurposeFn))
    }
}

#[derive(Debug, Clone)]
struct SetFieldPurposeFn;

impl Expression for SetFieldPurposeFn {
    fn resolve(&self, _ctx: &mut Context) -> Resolved {
        Ok(Value::Null)
    }

    fn type_def(&self, _state: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().null()
    }
}
