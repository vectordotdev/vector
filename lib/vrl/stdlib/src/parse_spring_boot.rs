use std::collections::BTreeMap;

// use datadog_grok::{parse_grok::parse_grok, parse_grok_rules::parse_grok_rules};
use ::value::Value;
use vrl::prelude::*;
use regex::Regex;

fn parse_spring_boot(value: Value) -> Resolved {
    let log = match &value {
        Value::Bytes(v) => {
            String::from_utf8_lossy(v)
        },
        _ => {
            String::from_utf8_lossy(&[])
        }
    }.to_string();

    let re = Regex::new(r"(?P<timestamp>\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}) (?P<loglevel>\w+) (?P<pid>\d+) --- \[\s*(?P<thread>\w+)\] (?P<logger>[^ ]+) : (?P<message>(.|\n|\t)+)").unwrap();
    let mut result = BTreeMap::new();

    match re.captures(log.as_str()) {
        Some(captures) => {
            let timestamp = captures.name("timestamp").unwrap().as_str();
            let level = captures.name("loglevel").unwrap().as_str();
            let pid = captures.name("pid").unwrap().as_str();
            let thread = captures.name("thread").unwrap().as_str();
            let logger = captures.name("logger").unwrap().as_str();
            let message = captures.name("message").unwrap().as_str();

            result.insert("timestamp".to_owned(), Value::from(timestamp));
            result.insert("level".to_owned(), Value::from(level));
            result.insert("pid".to_owned(), Value::from(pid));
            result.insert("thread".to_owned(),  Value::from(thread));
            result.insert("logger".to_owned(), Value::from(logger));
            result.insert("message".to_owned(), Value::from(message));
            Ok(result.into())
        },
        None => {
            result.insert("message".to_owned(), Value::from(log));
            Ok(result.into())
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ParseSpringBoot;

impl Function for ParseSpringBoot {
    fn identifier(&self) -> &'static str {
        "parse_spring_boot"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "parse spring boot",
            source: r#"parse_spring_boot("2023-01-30 22:37:33.495 INFO 72972 --- [ main] o.s.i.monitor.IntegrationMBeanExporter : Registering MessageChannel cacheConsumer-in-0
            ")"#,
            result: Ok(r#"
                {
                    "timestamp": "2023-01-30 22:37:33.495",
                    "level": "INFO",
                    "pid": 72972,
                    "thread": "main",
                    "logger": "o.s.i.monitor.IntegrationMBeanExporter",
                    "message": "Registering MessageChannel cacheConsumer-in-0"
                }
            "#),
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        Ok(ParseSpringBootFn { value }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct ParseSpringBootFn {
    value: Box<dyn Expression>,
}

impl FunctionExpression for ParseSpringBootFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?;
        parse_spring_boot(bytes)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::object(inner_kind())
    }
}

fn inner_kind() -> Collection<Field> {
    Collection::from_unknown(Kind::bytes().or_array(Collection::any()))
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        parse_spring_boot => ParseSpringBoot;

        normal {
            args: func_args![value: value!("2023-01-30 22:37:33.495 INFO 72972 --- [ main] o.s.i.monitor.IntegrationMBeanExporter : Registering MessageChannel cacheConsumer-in-0")],
            want: Ok(value!({
                timestamp: "2023-01-30 22:37:33.495",
                level: "INFO",
                pid: "72972",
                thread: "main",
                logger: "o.s.i.monitor.IntegrationMBeanExporter",
                message: "Registering MessageChannel cacheConsumer-in-0"
            })),
            tdef: TypeDef::object(inner_kind()),
        }

        error_trace {
            args: func_args![value: value!("2023-01-30 22:37:33.495 INFO 72972 --- [ main] o.s.i.monitor.IntegrationMBeanExporter : java.lang.NullPointerException: null\n\tat io.javabrains.EmployerController.getAllEmployers(EmployerController.java:20) ~[classes/:na]")],
            want: Ok(value!({
                timestamp: "2023-01-30 22:37:33.495",
                level: "INFO",
                pid: "72972",
                thread: "main",
                logger: "o.s.i.monitor.IntegrationMBeanExporter",
                message: "java.lang.NullPointerException: null\n\tat io.test.EmployerController.getAllEmployers(EmployerController.java:20) ~[classes/:na]"
            })),
            tdef: TypeDef::object(inner_kind()),
        }
    ];
}
