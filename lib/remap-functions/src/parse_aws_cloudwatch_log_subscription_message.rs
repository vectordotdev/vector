use remap::prelude::*;
use shared::{aws_cloudwatch_logs_subscription::AwsCloudWatchLogsSubscriptionMessage, btreemap};
use value::Kind;

#[derive(Clone, Copy, Debug)]
pub struct ParseAwsCloudWatchLogSubscriptionMessage;

impl Function for ParseAwsCloudWatchLogSubscriptionMessage {
    fn identifier(&self) -> &'static str {
        "parse_aws_cloudwatch_log_subscription_message"
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

        Ok(Box::new(ParseAwsCloudWatchLogSubscriptionMessageFn {
            value,
        }))
    }
}

#[derive(Debug, Clone)]
struct ParseAwsCloudWatchLogSubscriptionMessageFn {
    value: Box<dyn Expression>,
}

impl Expression for ParseAwsCloudWatchLogSubscriptionMessageFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.value.execute(state, object)?.try_bytes()?;

        let message = serde_json::from_slice::<AwsCloudWatchLogsSubscriptionMessage>(&bytes)
            .map_err(|e| format!("unable to parse: {}", e))?;

        Ok(btreemap! {
            "owner" => message.owner,
            "message_type" => message.message_type.as_str(),
            "log_group" => message.log_group,
            "log_stream" => message.log_stream,
            "subscription_filters" => message.subscription_filters,
            "log_events" => message.log_events.into_iter().map(|event| btreemap![
                "id" => event.id,
                "timestamp" => event.timestamp,
                "message" => event.message,
            ]).collect::<Vec<_>>(),
        }
        .into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .into_fallible(true) // Message parsing error
            .with_inner_type(inner_type_def())
            .with_constraint(value::Kind::Map)
    }
}

/// The type defs of the fields contained by the returned map.
fn inner_type_def() -> Option<InnerTypeDef> {
    Some(inner_type_def! ({
        "owner": Kind::Bytes,
        "message_type": Kind::Bytes,
        "log_group": Kind::Bytes,
        "log_stream": Kind::Bytes,
        "subscription_filters": TypeDef::from(Kind::Array)
            .with_inner_type(Some(inner_type_def!([ Kind::Bytes ]))),
        "log_events": TypeDef::from(Kind::Array)
            .with_inner_type(Some(inner_type_def! ({
                "id": Kind::Bytes,
                "timestamp": Kind::Timestamp,
                "message": Kind::Bytes,
            })))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use value::Kind;

    test_function![
        parse_aws_cloudwatch_log_subscription_message => ParseAwsCloudWatchLogSubscriptionMessage;

        invalid_type {
            args: func_args![value: "42"],
            want: Err("function call error: unable to parse: invalid type: integer `42`, expected struct AwsCloudWatchLogsSubscriptionMessage at line 1 column 2"),
        }

        string {
            args: func_args![value: r#"
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
"#],
            want: Ok(btreemap! {
                "owner" => "071959437513",
                "message_type" => "DATA_MESSAGE",
                "log_group" => "/jesse/test",
                "log_stream" => "test",
                "subscription_filters" => vec!["Destination"],
                "log_events" => vec![btreemap! {
                    "id" => "35683658089614582423604394983260738922885519999578275840",
                    "timestamp" => Utc.timestamp(1600110569, 39000000),
                    "message" => "{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-platform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}",
                }, btreemap! {
                    "id" => "35683658089659183914001456229543810359430816722590236673",
                    "timestamp" => Utc.timestamp(1600110569, 41000000),
                    "message" => "{\"bytes\":17707,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"109.81.244.252\",\"method\":\"GET\",\"protocol\":\"HTTP/2.0\",\"referer\":\"http://www.investormission-critical.io/24/7/vortals\",\"request\":\"/scale/functionalities/optimize\",\"source_type\":\"stdin\",\"status\":502,\"user-identifier\":\"feeney1708\"}",
                }],
            })
        }

        invalid_value {
            args: func_args![value: r#"{ INVALID }"#],
            want: Err("function call error: unable to parse: key must be a string at line 1 column 3"),
        }
    ];

    test_type_def![value_string {
        expr: |_| ParseAwsCloudWatchLogSubscriptionMessageFn {
            value: Literal::from("foo").boxed(),
        },
        def: TypeDef {
            fallible: true,
            kind: Kind::Map,
            inner_type_def: inner_type_def(),
        },
    }];
}
