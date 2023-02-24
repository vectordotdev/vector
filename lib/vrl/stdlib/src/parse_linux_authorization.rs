use vrl::prelude::*;

use crate::parse_syslog::ParseSyslogFn;

#[derive(Clone, Copy, Debug)]
pub struct ParseLinuxAuthorization;

impl Function for ParseLinuxAuthorization {
    fn identifier(&self) -> &'static str {
        "parse_linux_authorization"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "\
            parse authorization event",
            source: r#"parse_linux_authorization!(s'Mar 23 01:49:58 localhost sshd[1111]: Accepted publickey for eng from 10.1.1.1 port 8888 ssh2: RSA SHA256:foobar')"#,
            result: Ok(indoc! {r#"{
                "appname": "sshd",
                "hostname": "localhost",
                "message": "Accepted publickey for eng from 10.1.1.1 port 8888 ssh2: RSA SHA256:foobar",
                "procid": 1111,
                "timestamp": "2023-03-23T01:49:58Z"
            }"#}),
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        // The parse_linux_authorization function is just an alias for parse_syslog
        Ok(ParseSyslogFn { value }.as_expr())
    }
}
