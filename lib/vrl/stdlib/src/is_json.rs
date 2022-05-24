use ::value::Value;
use vrl::prelude::*;

fn is_json(bytes: &Bytes) -> bool {
    serde_json::from_slice::<'_, serde::de::IgnoredAny>(bytes).is_ok()
}

fn is_json_with_variant(bytes: &Bytes, variant: &Bytes) -> bool {
    match serde_json::from_slice::<'_, serde::de::IgnoredAny>(bytes) {
        Err(_) => false,
        Ok(_) => {
            for c in bytes {
                return match c {
                    // Search for the first non whitespace char
                    b' ' | b'\n' | b'\t' | b'\r' => continue,
                    b'{' => variant.as_ref() == b"object",
                    b'[' => variant.as_ref() == b"array",
                    b't' | b'f' => variant.as_ref() == b"bool",
                    b'-' | b'0'..=b'9' => variant.as_ref() == b"number",
                    b'"' => variant.as_ref() == b"string",
                    b'n' => variant.as_ref() == b"null",
                    _ => false,
                };
            }

            // Empty input value cannot be any type, not a specific variant
            false
        }
    }
}

fn variants() -> Vec<Value> {
    vec![
        value!("object"),
        value!("array"),
        value!("bool"),
        value!("number"),
        value!("string"),
        value!("null"),
    ]
}

#[derive(Clone, Copy, Debug)]
pub struct IsJson;

impl Function for IsJson {
    fn identifier(&self) -> &'static str {
        "is_json"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "variant",
                kind: kind::BYTES,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "object",
                source: r#"is_json("{}")"#,
                result: Ok("true"),
            },
            Example {
                title: "string",
                source: r#"is_json(s'"test"')"#,
                result: Ok("true"),
            },
            Example {
                title: "invalid",
                source: r#"is_json("}{")"#,
                result: Ok("false"),
            },
            Example {
                title: "exact_variant",
                source: r#"is_json("{}", variant: "object")"#,
                result: Ok("true"),
            },
        ]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let variant = arguments.optional_enum("variant", &variants())?;

        match variant {
            Some(raw_variant) => {
                let variant = raw_variant
                    .try_bytes()
                    .map_err(|e| Box::new(e) as Box<dyn DiagnosticMessage>)?;
                Ok(Box::new(IsJsonVariantsFn { value, variant }))
            }
            None => Ok(Box::new(IsJsonFn { value })),
        }
    }

    fn compile_argument(
        &self,
        _args: &[(&'static str, Option<FunctionArgument>)],
        _ctx: &mut FunctionCompileContext,
        name: &str,
        expr: Option<&expression::Expr>,
    ) -> CompiledArgument {
        match (name, expr) {
            ("variant", Some(expr)) => {
                let variant = expr
                    .as_enum("variant", variants())?
                    .try_bytes()
                    .map_err(|e| Box::new(e) as Box<dyn DiagnosticMessage>)?;

                Ok(Some(Box::new(variant) as _))
            }
            _ => Ok(None),
        }
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Result<Value> {
        let value = args.required("value");
        let variant = args.optional_any("variant");
        let bytes = value.try_as_bytes()?;

        match variant {
            Some(variant) => {
                let variant = variant.downcast_ref::<Bytes>().unwrap();
                Ok(is_json_with_variant(bytes, variant).into())
            }
            None => Ok(is_json(bytes).into()),
        }
    }
}

#[derive(Clone, Debug)]
struct IsJsonFn {
    value: Box<dyn Expression>,
}

impl Expression for IsJsonFn {
    fn resolve<'value, 'ctx: 'value, 'rt: 'ctx>(
        &'rt self,
        ctx: &'ctx mut Context,
    ) -> Resolved<'value> {
        let value = self.value.resolve(ctx)?;
        let bytes = value.try_as_bytes()?;

        Ok(Cow::Owned(is_json(bytes).into()))
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::boolean().infallible()
    }
}

#[derive(Clone, Debug)]
struct IsJsonVariantsFn {
    value: Box<dyn Expression>,
    variant: Bytes,
}

impl Expression for IsJsonVariantsFn {
    fn resolve<'value, 'ctx: 'value, 'rt: 'ctx>(
        &'rt self,
        ctx: &'ctx mut Context,
    ) -> Resolved<'value> {
        let value = self.value.resolve(ctx)?;
        let variant = &self.variant;
        let bytes = value.try_as_bytes()?;

        Ok(Cow::Owned(is_json_with_variant(bytes, variant).into()))
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::boolean().infallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        is_json => IsJson;

        object {
            args: func_args![value: r#"{}"#],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }

        string {
            args: func_args![value: r#""test""#],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }

        invalid {
            args: func_args![value: r#"}{"#],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }

        exact_variant {
            args: func_args![value: r#"{}"#, variant: "object"],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }

        exact_variant_invalid {
            args: func_args![value: r#"123"#, variant: "null"],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }

        variant_with_spaces {
            args: func_args![value: r#"   []"#, variant: "array"],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }

        invalid_variant {
            args: func_args![value: r#"[]"#, variant: "invalid-variant"],
            want: Err(r#"invalid enum variant""#),
            tdef: TypeDef::boolean().infallible(),
        }

        invalid_variant_type {
            args: func_args![value: r#"[]"#, variant: 100],
            want: Err(r#"invalid enum variant""#),
            tdef: TypeDef::boolean().infallible(),
        }
    ];
}
