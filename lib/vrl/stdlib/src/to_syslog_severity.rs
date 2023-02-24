use ::value::Value;
use vrl::prelude::*;

fn to_syslog_severity(level: Value) -> Resolved {
    let level = level.try_bytes_utf8_lossy()?;
    // Severity levels: https://en.wikipedia.org/wiki/Syslog#Severity_level
    let severity = match &level[..] {
        "emerg" | "panic" => 0,
        "alert" => 1,
        "crit" => 2,
        "err" | "error" => 3,
        "warning" | "warn" => 4,
        "notice" => 5,
        "info" => 6,
        "debug" => 7,
        _ => return Err(format!("syslog level {level} not valid").into()),
    };
    Ok(severity.into())
}

#[derive(Clone, Copy, Debug)]
pub struct ToSyslogSeverity;

impl Function for ToSyslogSeverity {
    fn identifier(&self) -> &'static str {
        "to_syslog_severity"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "valid",
                source: "to_syslog_severity!(s'crit')",
                result: Ok("2"),
            },
            Example {
                title: "invalid",
                source: "to_syslog_severity!(s'foobar')",
                result: Err(
                    r#"function call error for "to_syslog_severity" at (0:30): syslog level foobar not valid"#,
                ),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(ToSyslogSeverityFn { value }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct ToSyslogSeverityFn {
    value: Box<dyn Expression>,
}

impl FunctionExpression for ToSyslogSeverityFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let level = self.value.resolve(ctx)?;
        to_syslog_severity(level)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::integer().fallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        to_level => ToSyslogSeverity;

        emergency {
            args: func_args![value: value!("emerg")],
            want: Ok(value!(0)),
            tdef: TypeDef::integer().fallible(),
        }

        alert {
            args: func_args![value: value!("alert")],
            want: Ok(value!(1)),
            tdef: TypeDef::integer().fallible(),
        }

        critical {
            args: func_args![value: value!("crit")],
            want: Ok(value!(2)),
            tdef: TypeDef::integer().fallible(),
        }

        error {
            args: func_args![value: value!("err")],
            want: Ok(value!(3)),
            tdef: TypeDef::integer().fallible(),
        }

        warning {
            args: func_args![value: value!("warn")],
            want: Ok(value!(4)),
            tdef: TypeDef::integer().fallible(),
        }

        notice {
            args: func_args![value: value!("notice")],
            want: Ok(value!(5)),
            tdef: TypeDef::integer().fallible(),
        }

        informational {
            args: func_args![value: value!("info")],
            want: Ok(value!(6)),
            tdef: TypeDef::integer().fallible(),
        }

        debug {
            args: func_args![value: value!("debug")],
            want: Ok(value!(7)),
            tdef: TypeDef::integer().fallible(),
        }

        invalid_level_1 {
            args: func_args![value: value!("oopsie")],
            want: Err("syslog level oopsie not valid"),
            tdef: TypeDef::integer().fallible(),
        }

        invalid_level_2 {
            args: func_args![value: value!("aww schucks")],
            want: Err("syslog level aww schucks not valid"),
            tdef: TypeDef::integer().fallible(),
        }
    ];
}
