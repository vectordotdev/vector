use derivative::Derivative;
use serde::{Deserialize, Serialize};

use datadog_grok::parse_grok::parse_grok;
use vector_core::{
    event::{Event, VrlTarget},
    Result,
};
use vrl::diagnostic::Formatter;
use vrl::{Program, Runtime};

use crate::config::{DataType, GlobalOptions, TransformConfig, TransformDescription};
use crate::transforms::{FunctionTransform, Transform};

#[derive(Deserialize, Serialize, Debug, Clone, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct DataDogGrokConfig {
    pub field: Option<String>,
    pub helper_rules: Vec<String>,
    pub parsing_rules: Vec<String>,
}

inventory::submit! {
    TransformDescription::new::<DataDogGrokConfig>("datadog_grok_parser")
}

impl_generate_config_from_default!(DataDogGrokConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "datadog_grok_parser")]
impl TransformConfig for DataDogGrokConfig {
    async fn build(&self, _globals: &GlobalOptions) -> Result<Transform> {
        Grok::new(self.clone()).map(Transform::function)
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn output_type(&self) -> DataType {
        DataType::Any
    }

    fn transform_type(&self) -> &'static str {
        "datadog_grok_parser"
    }
}

#[derive(Debug, Clone)]
pub struct Grok {
    program: Program,
}

impl Grok {
    pub fn new(config: DataDogGrokConfig) -> crate::Result<Self> {
        let program = parse_grok(config.field, &config.helper_rules, &config.parsing_rules)
            .map_err(|diagnostics| {
                Formatter::new("", diagnostics) /*.colored()*/
                    .to_string()
            })?;

        Ok(Grok { program })
    }
}

impl FunctionTransform for Grok {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        let original_event = event.clone();

        let mut target: VrlTarget = event.into();

        let mut runtime = Runtime::default();

        let result = runtime.resolve(&mut target, &self.program);

        match result {
            Ok(_) => {
                for event in target.into_events() {
                    output.push(event)
                }
            }
            Err(e) => {
                // normally we shouldn't throw any runtime errors
                error!(message = "Unexpected runtime error.", %e);
                output.push(original_event);
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use shared::btreemap;
    use vector_core::event::{LogEvent, Value};

    use crate::transforms::test::transform_one;

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<DataDogGrokConfig>();
    }

    async fn parse_log(
        event: Event,
        helper_rules: Vec<String>,
        parsing_rules: Vec<String>,
        field: Option<String>,
    ) -> LogEvent {
        let metadata = event.metadata().clone();
        let mut parser = Grok::new(DataDogGrokConfig {
            field,
            helper_rules,
            parsing_rules,
        })
        .unwrap();

        let result = transform_one(&mut parser, event).unwrap().into_log();
        assert_eq!(result.metadata(), &metadata);
        result
    }

    #[tokio::test]
    async fn parses_with_one_parsing_rule() {
        let event = parse_log(
            Event::from(r##"127.0.0.1 - frank [13/Jul/2016:10:55:36 +0000] "GET /apache_pb.gif HTTP/1.0" 200 2326"##),
            // support rules
            vec![
                r#"_auth %{notSpace:http.auth:nullIf("-")}"#.to_string(),
                r#"_bytes_written %{integer:network.bytes_written}"#.to_string(),
                r#"_client_ip %{ipOrHost:network.client.ip}"#.to_string(),
                r#"_version HTTP\/(?<http.version>\d+\.\d+)"#.to_string(),
                r#"_url %{notSpace:http.url}"#.to_string(),
                r#"_ident %{notSpace:http.ident}"#.to_string(),
                r#"_user_agent %{regex("[^\\\"]*"):http.useragent}"#.to_string(),
                r#"_referer %{notSpace:http.referer}"#.to_string(),
                r#"_status_code %{integer:http.status_code}"#.to_string(),
                r#"_method %{word:http.method}"#.to_string(),
                r#"_date_access %{date("dd/MMM/yyyy:HH:mm:ss Z"):date_access}"#.to_string(),
                r#"_x_forwarded_for %{regex("[^\\\"]*"):http._x_forwarded_for:nullIf("-")}"#.to_string()],
            // match rules
            vec![
                r#"access.common %{_client_ip} %{_ident} %{_auth} \[%{_date_access}\] "(?>%{_method} |)%{_url}(?> %{_version}|)" %{_status_code} (?>%{_bytes_written}|-)"#.to_string(),
            ], None).await;

        assert_eq!(
            event["custom.date_access"],
            "13/Jul/2016:10:55:36 +0000".into()
        );
        assert_eq!(event["custom.http.auth"], "frank".into());
        assert_eq!(event["custom.http.method"], "GET".into());
        assert_eq!(event["custom.http.status_code"], 200.into());
        assert_eq!(event["custom.http.url"], "/apache_pb.gif".into());
        assert_eq!(event["custom.http.version"], "1.0".into());
        assert_eq!(event["custom.network.bytes_written"], 2326.into());
        assert_eq!(event["custom.network.client.ip"], "127.0.0.1".into());
    }

    #[tokio::test]
    async fn parses_with_multiple_parsing_rules() {
        let event = parse_log(
            Event::from(r##"127.0.0.1 - frank [13/Jul/2016:10:55:36 +0000] "GET /apache_pb.gif HTTP/1.0" 200 2326 0.202 "http://www.perdu.com/" "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/55.0.2883.87 Safari/537.36" "-""##),
            // helper rules
            vec![
                r#"_auth %{notSpace:http.auth:nullIf("-")}"#.to_string(),
                r#"_bytes_written %{integer:network.bytes_written}"#.to_string(),
                r#"_client_ip %{ipOrHost:network.client.ip}"#.to_string(),
                r#"_version HTTP\/(?<http.version>\d+\.\d+)"#.to_string(),
                r#"_url %{notSpace:http.url}"#.to_string(),
                r#"_ident %{notSpace:http.ident}"#.to_string(),
                r#"_user_agent %{regex("[^\\\"]*"):http.useragent}"#.to_string(),
                r#"_referer %{notSpace:http.referer}"#.to_string(),
                r#"_status_code %{integer:http.status_code}"#.to_string(),
                r#"_method %{word:http.method}"#.to_string(),
                r#"_date_access %{date("dd/MMM/yyyy:HH:mm:ss Z"):date_access}"#.to_string(),
                r#"_x_forwarded_for %{regex("[^\\\"]*"):http._x_forwarded_for:nullIf("-")}"#.to_string()],
            // parsing rules
            vec![
                r#"access.common %{_client_ip} %{_ident} %{_auth} \[%{_date_access}\] "(?>%{_method} |)%{_url}(?> %{_version}|)" %{_status_code} (?>%{_bytes_written}|-)"#.to_string(),
                r#"access.combined %{access.common} (%{number:duration:scale(1000000000)} )?"%{_referer}" "%{_user_agent}"( "%{_x_forwarded_for}")?.*"#.to_string()
            ], None).await;

        assert_eq!(
            event["custom.date_access"],
            "13/Jul/2016:10:55:36 +0000".into()
        );
        assert_eq!(event["custom.duration"], 202000000.0.into());
        assert_eq!(event["custom.http.auth"], "frank".into());
        assert_eq!(event["custom.http.method"], "GET".into());
        assert_eq!(event["custom.http.status_code"], 200.into());
        assert_eq!(event["custom.http.url"], "/apache_pb.gif".into());
        assert_eq!(event["custom.http.version"], "1.0".into());
        assert_eq!(event["custom.http.referer"], "http://www.perdu.com/".into());
        assert_eq!(event["custom.http.useragent"], "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/55.0.2883.87 Safari/537.36".into());
        assert_eq!(event["custom.http._x_forwarded_for"], Value::Null);
        assert_eq!(event["custom.network.bytes_written"], 2326.into());
        assert_eq!(event["custom.network.client.ip"], "127.0.0.1".into());
    }

    async fn test_grok_pattern(tests: Vec<(&str, &str, Option<Value>)>) {
        for (filter, k, v) in tests {
            let event = parse_log(
                Event::from(k),
                vec![],
                vec![format!(r#"test {}"#, filter)],
                None,
            )
            .await;

            assert_eq!(event.get("custom.field"), v.as_ref());
        }
    }
}
