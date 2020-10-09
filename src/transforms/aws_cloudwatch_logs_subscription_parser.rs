use super::Transform;
use crate::{
    config::{
        log_schema, DataType, GenerateConfig, TransformConfig, TransformContext,
        TransformDescription,
    },
    event::Event,
    internal_events::{
        AwsCloudwatchLogsSubscriptionParserEventProcessed,
        AwsCloudwatchLogsSubscriptionParserFailedParse,
    },
};
use chrono::{serde::ts_milliseconds, DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::iter;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Clone, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct AwsCloudwatchLogsSubscriptionParserConfig {
    pub field: Option<Atom>,
}

inventory::submit! {
    TransformDescription::new::<AwsCloudwatchLogsSubscriptionParserConfig>("aws_cloudwatch_logs_subscription_parser")
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_cloudwatch_logs_subscription_parser")]
impl TransformConfig for AwsCloudwatchLogsSubscriptionParserConfig {
    async fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(AwsCloudwatchLogsSubscriptionParser::from(
            self.clone(),
        )))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "aws_cloudwatch_logs_subscription_parser"
    }
}

impl GenerateConfig for AwsCloudwatchLogsSubscriptionParserConfig {}

#[derive(Debug)]
pub struct AwsCloudwatchLogsSubscriptionParser {
    field: Atom,
}

impl From<AwsCloudwatchLogsSubscriptionParserConfig> for AwsCloudwatchLogsSubscriptionParser {
    fn from(
        config: AwsCloudwatchLogsSubscriptionParserConfig,
    ) -> AwsCloudwatchLogsSubscriptionParser {
        AwsCloudwatchLogsSubscriptionParser {
            field: config
                .field
                .unwrap_or_else(|| Atom::from(log_schema().message_key())),
        }
    }
}

impl Transform for AwsCloudwatchLogsSubscriptionParser {
    fn transform(&mut self, _event: Event) -> Option<Event> {
        // required for trait, but transform_into is used instead
        unimplemented!()
    }

    fn transform_into(&mut self, output: &mut Vec<Event>, event: Event) {
        let log = event.as_log();

        emit!(AwsCloudwatchLogsSubscriptionParserEventProcessed);

        let message = log
            .get(&self.field)
            .map(|s| s.as_bytes())
            .and_then(|to_parse| {
                serde_json::from_slice::<AwsCloudWatchLogsSubscriptionMessage>(&to_parse)
                    .map_err(|error| {
                        emit!(AwsCloudwatchLogsSubscriptionParserFailedParse { error })
                    })
                    .ok()
            });

        let events = message
            .map(|m| subscription_event_to_events(&event, m))
            .unwrap_or_else(|| Box::new(iter::empty()));

        output.extend(events);
    }
}

fn subscription_event_to_events<'a>(
    event: &'a Event,
    message: AwsCloudWatchLogsSubscriptionMessage,
) -> Box<dyn Iterator<Item = Event> + 'a> {
    match message.message_type {
        AwsCloudWatchLogsSubscriptionMessageType::ControlMessage => {
            Box::new(iter::empty::<Event>()) as Box<dyn Iterator<Item = Event> + 'a>
        }
        AwsCloudWatchLogsSubscriptionMessageType::DataMessage => {
            let log_group = message.log_group;
            let log_stream = message.log_stream;
            let owner = message.owner;
            let subscription_filters = message.subscription_filters;

            Box::new(message.log_events.into_iter().map(move |log_event| {
                let mut event = event.clone();
                let log = event.as_mut_log();

                log.insert(log_schema().message_key(), log_event.message);
                log.insert(log_schema().timestamp_key(), log_event.timestamp);
                log.insert("id", log_event.id);
                log.insert("log_group", log_group.clone());
                log.insert("log_stream", log_stream.clone());
                log.insert("subscription_filters", subscription_filters.clone());
                log.insert("owner", owner.clone());

                event
            })) as Box<dyn Iterator<Item = Event> + 'a>
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum AwsCloudWatchLogsSubscriptionMessageType {
    ControlMessage,
    DataMessage,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AwsCloudWatchLogsSubscriptionMessage {
    owner: String,
    message_type: AwsCloudWatchLogsSubscriptionMessageType,
    log_group: String,
    log_stream: String,
    subscription_filters: Vec<String>,
    log_events: Vec<AwsCloudWatchLogEvent>,
}

#[derive(Debug, Deserialize)]
struct AwsCloudWatchLogEvent {
    id: String,
    #[serde(with = "ts_milliseconds")]
    timestamp: DateTime<Utc>,
    message: String,
}

#[cfg(test)]
mod test {
    use super::{AwsCloudwatchLogsSubscriptionParser, AwsCloudwatchLogsSubscriptionParserConfig};
    use crate::{event::Event, event::LogEvent, log_event, transforms::Transform};
    use chrono::{TimeZone, Utc};
    use pretty_assertions::assert_eq;

    #[test]
    fn aws_cloudwatch_logs_subscription_parser_emits_events() {
        let mut parser =
            AwsCloudwatchLogsSubscriptionParser::from(AwsCloudwatchLogsSubscriptionParserConfig {
                field: None,
            });

        let mut event = Event::from(
            r#"
{
  "messageType": "DATA_MESSAGE",
  "owner": "071959437513",
  "logGroup": "/jesse/test",
  "logStream": "test",
  "subscriptionFilters": [
    "Destination"
  ],
  "logEvents": [
    {
      "id": "35683658089614582423604394983260738922885519999578275840",
      "timestamp": 1600110569039,
      "message": "{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-platform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}"
    },
    {
      "id": "35683658089659183914001456229543810359430816722590236673",
      "timestamp": 1600110569041,
      "message": "{\"bytes\":17707,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"109.81.244.252\",\"method\":\"GET\",\"protocol\":\"HTTP/2.0\",\"referer\":\"http://www.investormission-critical.io/24/7/vortals\",\"request\":\"/scale/functionalities/optimize\",\"source_type\":\"stdin\",\"status\":502,\"user-identifier\":\"feeney1708\"}"
    }
  ]
}
"#,
        );
        let log = event.as_mut_log();
        log.insert("keep", "field");

        let mut output: Vec<Event> = Vec::new();

        parser.transform_into(&mut output, event);

        assert_eq!(
            output,
            vec![
                log_event! {
                    "id" => "35683658089614582423604394983260738922885519999578275840",
                    "message"=> r#"{"bytes":26780,"datetime":"14/Sep/2020:11:45:41 -0400","host":"157.130.216.193","method":"PUT","protocol":"HTTP/1.0","referer":"https://www.principalcross-platform.io/markets/ubiquitous","request":"/expedite/convergence","source_type":"stdin","status":301,"user-identifier":"-"}"#,
                    "timestamp" => Utc.timestamp(1600110569, 39000000),
                    "log_group" => "/jesse/test",
                    "log_stream" => "test",
                    "owner" => "071959437513",
                    "subscription_filters" => vec![ "Destination" ],
                    "keep" => "field",
                },
                log_event! {
                    "id" => "35683658089659183914001456229543810359430816722590236673",
                    "message" => r#"{"bytes":17707,"datetime":"14/Sep/2020:11:45:41 -0400","host":"109.81.244.252","method":"GET","protocol":"HTTP/2.0","referer":"http://www.investormission-critical.io/24/7/vortals","request":"/scale/functionalities/optimize","source_type":"stdin","status":502,"user-identifier":"feeney1708"}"#,
                    "timestamp" => Utc.timestamp(1600110569, 41000000),
                    "log_group" => "/jesse/test",
                    "log_stream" => "test",
                    "owner" => "071959437513",
                    "subscription_filters" => vec![ "Destination" ],
                    "keep" => "field",
                },
            ]
        )
    }

    #[test]
    fn aws_cloudwatch_logs_subscription_parser_ignores_control_messages() {
        let mut parser =
            AwsCloudwatchLogsSubscriptionParser::from(AwsCloudwatchLogsSubscriptionParserConfig {
                field: None,
            });

        let event = Event::from(
            r#"
{
  "messageType": "CONTROL_MESSAGE",
  "owner": "CloudwatchLogs",
  "logGroup": "",
  "logStream": "",
  "subscriptionFilters": [],
  "logEvents": [
    {
      "id": "",
      "timestamp": 1600110003794,
      "message": "CWL CONTROL MESSAGE: Checking health of destination Firehose."
    }
  ]
}
"#,
        );

        let mut output: Vec<Event> = Vec::new();

        parser.transform_into(&mut output, event);

        assert_eq!(output, vec![]);
    }
}
