use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToSyslogSeverity;

impl Function for ToSyslogSeverity {
    fn identifier(&self) -> &'static str {
        "to_syslog_severity"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        Ok(Box::new(ToSyslogSeverityFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ToSyslogSeverityFn {
    value: Box<dyn Expression>,
}

impl Expression for ToSyslogSeverityFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let level_bytes = self.value.execute(state, object)?.try_bytes()?;

        let level = String::from_utf8_lossy(&level_bytes);

        // Severity levels: https://en.wikipedia.org/wiki/Syslog#Severity_level
        let severity = match &level[..] {
            "emerg" | "panic" => Ok(0),
            "alert" => Ok(1),
            "crit" => Ok(2),
            "err" | "error" => Ok(3),
            "warning" | "warn" => Ok(4),
            "notice" => Ok(5),
            "info" => Ok(6),
            "debug" => Ok(7),
            _ => Err(format!("level {} not valid", level)),
        };

        match severity {
            Ok(severity) => Ok(Value::from(severity)),
            Err(e) => Err(Error::from(e)),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        self.value
            .type_def(state)
            .fallible_unless(Kind::Bytes)
            .with_constraint(Kind::Integer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use value::Kind;

    test_type_def![
        value_string_infallible {
            expr: |_| ToSyslogSeverityFn {
                value: Literal::from("warning").boxed(),
            },
            def: TypeDef { fallible: false, kind: Kind::Integer, ..Default::default() },
        }

        value_not_string_fallible {
            expr: |_| ToSyslogSeverityFn {
                value: Literal::from(27).boxed(),
            },
            def: TypeDef { fallible: true, kind: Kind::Integer, ..Default::default() },
        }
    ];

    test_function![
        to_level => ToSyslogSeverity;

        emergency {
            args: func_args![value: value!("emerg")],
            want: Ok(value!(0)),
        }

        alert {
            args: func_args![value: value!("alert")],
            want: Ok(value!(1)),
        }

        critical {
            args: func_args![value: value!("crit")],
            want: Ok(value!(2)),
        }

        error {
            args: func_args![value: value!("err")],
            want: Ok(value!(3)),
        }

        warning {
            args: func_args![value: value!("warn")],
            want: Ok(value!(4)),
        }

        notice {
            args: func_args![value: value!("notice")],
            want: Ok(value!(5)),
        }

        informational {
            args: func_args![value: value!("info")],
            want: Ok(value!(6)),
        }

        debug {
            args: func_args![value: value!("debug")],
            want: Ok(value!(7)),
        }

        invalid_level_1 {
            args: func_args![value: value!("oopsie")],
            want: Err("function call error: level oopsie not valid"),
        }

        invalid_level_2 {
            args: func_args![value: value!("aww schucks")],
            want: Err("function call error: level aww schucks not valid"),
        }
    ];
}
