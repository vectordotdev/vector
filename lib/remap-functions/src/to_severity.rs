use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToSeverity;

impl Function for ToSeverity {
    fn identifier(&self) -> &'static str {
        "to_severity"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "level",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let level = arguments.required("level")?.boxed();

        Ok(Box::new(ToSeverityFn { level }))
    }
}

#[derive(Debug, Clone)]
struct ToSeverityFn {
    level: Box<dyn Expression>,
}

impl Expression for ToSeverityFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let level_bytes = self.level.execute(state, object)?.try_bytes()?;

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
            Err(e) => Err(Error::Call(e)),
        }
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef {
            kind: value::Kind::Integer,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        to_level => ToSeverity;

        emergency {
            args: func_args![level: value!("emerg")],
            want: Ok(value!(0)),
        }

        alert {
            args: func_args![level: value!("alert")],
            want: Ok(value!(1)),
        }

        critical {
            args: func_args![level: value!("crit")],
            want: Ok(value!(2)),
        }

        error {
            args: func_args![level: value!("err")],
            want: Ok(value!(3)),
        }

        warning {
            args: func_args![level: value!("warn")],
            want: Ok(value!(4)),
        }

        notice {
            args: func_args![level: value!("notice")],
            want: Ok(value!(5)),
        }

        informational {
            args: func_args![level: value!("info")],
            want: Ok(value!(6)),
        }

        debug {
            args: func_args![level: value!("debug")],
            want: Ok(value!(7)),
        }

        invalid_level_1 {
            args: func_args![level: value!("oopsie")],
            want: Err("function call error: level oopsie not valid"),
        }

        invalid_level_2 {
            args: func_args![level: value!("aww shucks")],
            want: Err("function call error:: level aww schucks not valid"),
        }
    ];
}
