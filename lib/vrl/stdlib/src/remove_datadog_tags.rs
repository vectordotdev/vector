use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct RemoveDatadogTags;

impl Function for RemoveDatadogTags {
    fn identifier(&self) -> &'static str {
        "remove_datadog_tags"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "tags",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "key",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let tags = arguments.required("tags");
        let key = arguments.required("key");
        let value = arguments.optional("value");

        Ok(Box::new(RemoveDatadogTagsFn { tags, key, value }))
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "remove datadog tags",
            source: r#"remove_datadog_tags!("env:beta,env:local,platform:windows", "env")"#,
            result: Ok("platform:windows"),
        }]
    }
}

#[derive(Debug, Clone)]
struct RemoveDatadogTagsFn {
    tags: Box<dyn Expression>,
    key: Box<dyn Expression>,
    value: Option<Box<dyn Expression>>,
}

impl Expression for RemoveDatadogTagsFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let tags = self.tags.resolve(ctx)?;
        let current_tags = tags.try_bytes_utf8_lossy()?;
        let key = self.key.resolve(ctx)?.try_bytes_utf8_lossy()?.into_owned();
        let tag_to_remove;
        let f: Box<dyn Fn(&str) -> bool> = match &self.value {
            None => {
                tag_to_remove = format!("{}:", key);
                Box::new(|t: &str| !t.starts_with(&tag_to_remove))
            }
            Some(value) => {
                let value = value.resolve(ctx)?.try_bytes_utf8_lossy()?.into_owned();
                tag_to_remove = format!("{}:{}", key, value);
                Box::new(|t: &str| t != tag_to_remove)
            }
        };

        let current_tags_vec = current_tags.split(',').filter(|t| f(t)).collect::<Vec<_>>();
        let final_tags = current_tags_vec.join(",");
        Ok(Value::from(final_tags))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod test {
        use super::*;
        test_function![
            remove_datadog_tags => RemoveDatadogTags;

            with_key {
                args: func_args![tags: "env:prod,arch:arm64", key: "arch"],
                want: Ok(value!("env:prod")),
                tdef: TypeDef::new().fallible().bytes(),
            }

            with_key_and_multiple_match {
                args: func_args![tags: "env:prod,arch:arm64,arch:arm", key: "arch"],
                want: Ok(value!("env:prod")),
                tdef: TypeDef::new().fallible().bytes(),
            }

            with_key_value {
                args: func_args![tags: "env:prod,arch:arm64", key: "arch", value: "arm64"],
                want: Ok(value!("env:prod")),
                tdef: TypeDef::new().fallible().bytes(),
            }
        ];
    }
}
