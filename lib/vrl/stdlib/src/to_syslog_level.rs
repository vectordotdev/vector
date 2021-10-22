use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToSyslogLevel;

impl Function for ToSyslogLevel {
    fn identifier(&self) -> &'static str {
        "to_syslog_level"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::INTEGER,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "valid",
                source: "to_syslog_level!(0)",
                result: Ok("emerg"),
            },
            Example {
                title: "invalid",
                source: "to_syslog_level!(500)",
                result: Err(
                    r#"function call error for "to_syslog_level" at (0:21): severity level 500 not valid"#,
                ),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ToSyslogLevelFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ToSyslogLevelFn {
    value: Box<dyn Expression>,
}

impl Expression for ToSyslogLevelFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?.try_integer()?;

        // Severity levels: https://en.wikipedia.org/wiki/Syslog#Severity_level
        let level = match value {
            0 => "emerg",
            1 => "alert",
            2 => "crit",
            3 => "err",
            4 => "warning",
            5 => "notice",
            6 => "info",
            7 => "debug",
            _ => return Err(format!("severity level {} not valid", value).into()),
        };

        Ok(level.into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        to_syslog_level => ToSyslogLevel;

        emergency {
            args: func_args![value: value!(0)],
            want: Ok(value!("emerg")),
            tdef: TypeDef::new().fallible().bytes(),
        }

        alert {
            args: func_args![value: value!(1)],
            want: Ok(value!("alert")),
            tdef: TypeDef::new().fallible().bytes(),
        }

        critical {
            args: func_args![value: value!(2)],
            want: Ok(value!("crit")),
            tdef: TypeDef::new().fallible().bytes(),
        }

        error {
            args: func_args![value: value!(3)],
            want: Ok(value!("err")),
            tdef: TypeDef::new().fallible().bytes(),
        }

        warning {
            args: func_args![value: value!(4)],
            want: Ok(value!("warning")),
            tdef: TypeDef::new().fallible().bytes(),
        }

        notice {
            args: func_args![value: value!(5)],
            want: Ok(value!("notice")),
            tdef: TypeDef::new().fallible().bytes(),
        }

        informational {
            args: func_args![value: value!(6)],
            want: Ok(value!("info")),
            tdef: TypeDef::new().fallible().bytes(),
        }

        debug {
            args: func_args![value: value!(7)],
            want: Ok(value!("debug")),
            tdef: TypeDef::new().fallible().bytes(),
        }

        invalid_severity_next_int {
            args: func_args![value: value!(8)],
            want: Err("severity level 8 not valid"),
            tdef: TypeDef::new().fallible().bytes(),
        }

        invalid_severity_larger_int {
            args: func_args![value: value!(475)],
            want: Err("severity level 475 not valid"),
            tdef: TypeDef::new().fallible().bytes(),
        }

        invalid_severity_negative_int {
            args: func_args![value: value!(-1)],
            want: Err("severity level -1 not valid"),
            tdef: TypeDef::new().fallible().bytes(),
        }
    ];
}
