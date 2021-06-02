use std::borrow::Cow;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct AddDatadogTags;

impl Function for AddDatadogTags {
    fn identifier(&self) -> &'static str {
        "add_datadog_tags"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "tags",
                kind: kind::ARRAY,
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let tags = arguments.required("tags");

        Ok(Box::new(AddDatadogTagsFn {
            value,
            tags
        }))
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "add datadog tags",
                source: r#"add_datadog_tags!("env:beta,platform:windows", ["arch:amd64", "relay:vector"])"#,
                result: Ok("env:beta,platform:windows,arch:amd64,relay:vector"),
            },
        ]
    }
}

#[derive(Debug, Clone)]
struct AddDatadogTagsFn {
    value: Box<dyn Expression>,
    tags: Box<dyn Expression>,
}

impl Expression for AddDatadogTagsFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let new_tags = self.tags.resolve(ctx)?.try_array()?;
        let mut new_tags_vec = new_tags
            .iter()
            .map(|s| s.try_bytes_utf8_lossy().map_err(Into::into))
            .collect::<Result<Vec<Cow<'_, str>>>>()
            .map_err(|_| "all tags items must be strings")?;


        let value = self.value.resolve(ctx)?;
        let current_tags = value.try_bytes_utf8_lossy()?;
        let mut current_tags_vec = current_tags.split(',')
            .filter(|&s| !s.is_empty())
            .map(|s| Cow::from(s))
            .collect::<Vec<_>>();

        current_tags_vec.append(&mut new_tags_vec);
        // Remove duplicates
        current_tags_vec.sort();
        current_tags_vec.dedup();

        let final_tags =  current_tags_vec
            .join(",");

        Ok(Value::from(final_tags))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod test {
        use super::*;
        test_function![
            add_datadog_tags => AddDatadogTags;

            with_reordering {
                args: func_args![value: "env:prod", tags: value!(["arch:arm64", "os:windows"])],
                want: Ok(value!("arch:arm64,env:prod,os:windows")),
                tdef: TypeDef::new().infallible().bytes(),
            }

            with_duplicate {
                args: func_args![value: "env:prod", tags: value!(["env:prod"])],
                want: Ok(value!("env:prod")),
                tdef: TypeDef::new().infallible().bytes(),
            }
        ];
    }

}
