use std::collections::BTreeMap;

use ::value::Value;
use vector_common::aws_cloudwatch_logs_subscription::AwsCloudWatchLogsSubscriptionMessage;
use vrl::prelude::*;

fn parse_aws_cloudwatch_log_subscription_message(bytes: Value) -> Resolved {
    let bytes = bytes.try_bytes()?;
    let message = serde_json::from_slice::<AwsCloudWatchLogsSubscriptionMessage>(&bytes)
        .map_err(|e| format!("unable to parse: {e}"))?;
    let map = Value::from(BTreeMap::from([
        (String::from("owner"), Value::from(message.owner)),
        (
            String::from("message_type"),
            Value::from(message.message_type.as_str()),
        ),
        (String::from("log_group"), Value::from(message.log_group)),
        (String::from("log_stream"), Value::from(message.log_stream)),
        (
            String::from("subscription_filters"),
            Value::from(message.subscription_filters),
        ),
        (
            String::from("log_events"),
            Value::Array(
                message
                    .log_events
                    .into_iter()
                    .map(|event| {
                        Value::from(BTreeMap::from([
                            (String::from("id"), Value::from(event.id)),
                            (String::from("timestamp"), Value::from(event.timestamp)),
                            (String::from("message"), Value::from(event.message)),
                        ]))
                    })
                    .collect::<Vec<Value>>(),
            ),
        ),
    ]));
    Ok(map)
}

#[derive(Clone, Copy, Debug)]
pub struct ParseAwsCloudWatchLogSubscriptionMessage;

impl Function for ParseAwsCloudWatchLogSubscriptionMessage {
    fn identifier(&self) -> &'static str {
        "parse_aws_cloudwatch_log_subscription_message"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "valid",
            source: indoc! {r#"
                parse_aws_cloudwatch_log_subscription_message!(s'{
                    "messageType": "DATA_MESSAGE",
                    "owner": "111111111111",
                    "logGroup": "test",
                    "logStream": "test",
                    "subscriptionFilters": [
                        "Destination"
                    ],
                    "logEvents": [
                        {
                            "id": "35683658089614582423604394983260738922885519999578275840",
                            "timestamp": 1600110569039,
                            "message": "{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41-0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-platform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}"
                        }
                    ]
                }')
            "#},
            result: Ok(indoc! {r#"{
                "log_events": [{
                    "id": "35683658089614582423604394983260738922885519999578275840",
                    "message": "{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41-0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-platform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}",
                    "timestamp": "2020-09-14T19:09:29.039Z"}
                ],
                "log_group": "test",
                "log_stream": "test",
                "message_type": "DATA_MESSAGE",
                "owner": "111111111111",
                "subscription_filters": ["Destination"]
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

        Ok(ParseAwsCloudWatchLogSubscriptionMessageFn { value }.as_expr())
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }
}

#[derive(Debug, Clone)]
struct ParseAwsCloudWatchLogSubscriptionMessageFn {
    value: Box<dyn Expression>,
}

impl FunctionExpression for ParseAwsCloudWatchLogSubscriptionMessageFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?;
        parse_aws_cloudwatch_log_subscription_message(bytes)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::object(inner_kind()).fallible(/* message parsing error */)
    }
}

fn inner_kind() -> BTreeMap<Field, Kind> {
    BTreeMap::from([
        (Field::from("owner"), Kind::bytes()),
        (Field::from("message_type"), Kind::bytes()),
        (Field::from("log_group"), Kind::bytes()),
        (Field::from("log_stream"), Kind::bytes()),
        (
            Field::from("subscription_filters"),
            Kind::array({
                let mut v = Collection::any();
                v.set_unknown(Kind::bytes());
                v
            }),
        ),
        (
            Field::from("log_events"),
            Kind::array(Collection::from_unknown(Kind::object(BTreeMap::from([
                (Field::from("id"), Kind::bytes()),
                (Field::from("timestamp"), Kind::timestamp()),
                (Field::from("message"), Kind::bytes()),
            ])))),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    test_function![
        parse_aws_cloudwatch_log_subscription_message => ParseAwsCloudWatchLogSubscriptionMessage;

        invalid_type {
            args: func_args![value: "42"],
            want: Err("unable to parse: invalid type: integer `42`, expected struct AwsCloudWatchLogsSubscriptionMessage at line 1 column 2"),
            tdef: TypeDef::object(inner_kind()).fallible(),
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
            want: Ok(Value::from(BTreeMap::from([
                (String::from("owner"), Value::from("071959437513")),
                (String::from("message_type"), Value::from("DATA_MESSAGE")),
                (String::from("log_group"), Value::from("/jesse/test")),
                (String::from("log_stream"), Value::from("test")),
                (String::from("subscription_filters"), Value::from(vec![Value::from("Destination")])),
                (String::from("log_events"), Value::from(vec![Value::from(BTreeMap::from([
                    (String::from("id"), Value::from( "35683658089614582423604394983260738922885519999578275840")),
                    (String::from("timestamp"), Value::from(Utc.timestamp_opt(1_600_110_569, 39_000_000).single().expect("invalid timestamp"))),
                    (String::from("message"), Value::from("{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-platform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}")),
                ])), Value::from(BTreeMap::from([
                    (String::from("id"), Value::from("35683658089659183914001456229543810359430816722590236673")),
                    (String::from("timestamp"), Value::from(Utc.timestamp_opt(1_600_110_569, 41_000_000).single().expect("invalid timestamp"))),
                    (String::from("message"), Value::from("{\"bytes\":17707,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"109.81.244.252\",\"method\":\"GET\",\"protocol\":\"HTTP/2.0\",\"referer\":\"http://www.investormission-critical.io/24/7/vortals\",\"request\":\"/scale/functionalities/optimize\",\"source_type\":\"stdin\",\"status\":502,\"user-identifier\":\"feeney1708\"}")),
                ]))])),
                ]))),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        invalid_value {
            args: func_args![value: r#"{ INVALID }"#],
            want: Err("unable to parse: key must be a string at line 1 column 3"),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }
    ];
}
