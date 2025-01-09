use crate::parser::DnstapParser;
use crate::schema::DnstapEventSchema;
use base64::prelude::{Engine as _, BASE64_STANDARD};
use dnsmsg_parser::dns_message_parser::DnsParserOptions;
use vector_lib::event::LogEvent;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ParseDnstap;

impl Function for ParseDnstap {
    fn identifier(&self) -> &'static str {
        "parse_dnstap"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "lowercase_hostnames",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "Parse dnstap query message",
            source: r#"parse_dnstap!("ChVqYW1lcy1WaXJ0dWFsLU1hY2hpbmUSC0JJTkQgOS4xNi4zGgBy5wEIAxACGAEiEAAAAAAAAAAAAAAAAAAAAAAqECABBQJwlAAAAAAAAAAAADAw8+0CODVA7+zq9wVNMU3WNlI2kwIAAAABAAAAAAABCWZhY2Vib29rMQNjb20AAAEAAQAAKQIAAACAAAAMAAoACOxjCAG9zVgzWgUDY29tAGAAbQAAAAByZLM4AAAAAQAAAAAAAQJoNQdleGFtcGxlA2NvbQAABgABAAApBNABAUAAADkADwA1AAlubyBTRVAgbWF0Y2hpbmcgdGhlIERTIGZvdW5kIGZvciBkbnNzZWMtZmFpbGVkLm9yZy54AQ==")"#,
            result: Ok(indoc!(
                r#"{
                        "dataType": "Message",
                        "dataTypeId": 1,
                        "extraInfo": "",
                        "messageType": "ResolverQuery",
                        "messageTypeId": 3,
                        "queryZone": "com.",
                        "requestData": {
                            "fullRcode": 0,
                            "header": {
                                "aa": false,
                                "ad": false,
                                "anCount": 0,
                                "arCount": 1,
                                "cd": false,
                                "id": 37634,
                                "nsCount": 0,
                                "opcode": 0,
                                "qdCount": 1,
                                "qr": 0,
                                "ra": false,
                                "rcode": 0,
                                "rd": false,
                                "tc": false
                            },
                            "opt": {
                                "do": true,
                                "ednsVersion": 0,
                                "extendedRcode": 0,
                                "options": [
                                    {
                                        "optCode": 10,
                                        "optName": "Cookie",
                                        "optValue": "7GMIAb3NWDM="
                                    }
                                ],
                                "udpPayloadSize": 512
                            },
                            "question": [
                                {
                                    "class": "IN",
                                    "domainName": "facebook1.com.",
                                    "questionType": "A",
                                    "questionTypeId": 1
                                }
                            ],
                            "rcodeName": "NoError"
                        },
                        "responseData": {
                            "fullRcode": 16,
                            "header": {
                                "aa": false,
                                "ad": false,
                                "anCount": 0,
                                "arCount": 1,
                                "cd": false,
                                "id": 45880,
                                "nsCount": 0,
                                "opcode": 0,
                                "qdCount": 1,
                                "qr": 0,
                                "ra": false,
                                "rcode": 16,
                                "rd": false,
                                "tc": false
                            },
                            "opt": {
                                "do": false,
                                "ednsVersion": 1,
                                "extendedRcode": 1,
                                "ede": [
                                    {
                                        "extraText": "no SEP matching the DS found for dnssec-failed.org.",
                                        "infoCode": 9,
                                        "purpose": "DNSKEY Missing"
                                    }
                                ],
                                "udpPayloadSize": 1232
                            },
                            "question": [
                                {
                                    "class": "IN",
                                    "domainName": "h5.example.com.",
                                    "questionType": "SOA",
                                    "questionTypeId": 6
                                }
                            ],
                            "rcodeName": "BADSIG"
                        },
                        "responseAddress": "2001:502:7094::30",
                        "responsePort": 53,
                        "serverId": "james-Virtual-Machine",
                        "serverVersion": "BIND 9.16.3",
                        "socketFamily": "INET6",
                        "socketProtocol": "UDP",
                        "sourceAddress": "::",
                        "sourcePort": 46835,
                        "time": 1593489007920014129,
                        "timePrecision": "ns",
                        "timestamp": "2020-06-30T03:50:07.920014129Z"
                    }"#
            )),
        }]
    }

    fn compile(
        &self,
        _state: &TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let lowercase_hostnames = arguments
            .optional("lowercase_hostnames")
            .unwrap_or_else(|| expr!(false));
        Ok(ParseDnstapFn {
            value,
            lowercase_hostnames,
        }
        .as_expr())
    }
}

#[derive(Debug, Clone)]
struct ParseDnstapFn {
    value: Box<dyn Expression>,
    lowercase_hostnames: Box<dyn Expression>,
}

impl FunctionExpression for ParseDnstapFn {
    fn resolve(&self, ctx: &mut Context<'_>) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let input = value.try_bytes_utf8_lossy()?;

        let mut event = LogEvent::default();

        DnstapParser::parse(
            &mut event,
            BASE64_STANDARD
                .decode(input.as_bytes())
                .map_err(|_| format!("{input} is not a valid base64 encoded string"))?
                .into(),
            DnsParserOptions {
                lowercase_hostnames: self.lowercase_hostnames.resolve(ctx)?.try_boolean()?,
            },
        )
        .map_err(|e| format!("dnstap parsing failed for {input}: {e}"))?;

        Ok(event.value().clone())
    }

    fn type_def(&self, _: &TypeState) -> TypeDef {
        TypeDef::object(DnstapEventSchema.request_message_schema_definition()).fallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone, Utc};
    use vrl::value;

    test_function![
        parse_dnstap => ParseDnstap;

        query {
            args: func_args![value: value!("ChVqYW1lcy1WaXJ0dWFsLU1hY2hpbmUSC0JJTkQgOS4xNi4zGgBy5wEIAxACGAEiEAAAAAAAAAAAAAAAAAAAAAAqECABBQJwlAAAAAAAAAAAADAw8+0CODVA7+zq9wVNMU3WNlI2kwIAAAABAAAAAAABCWZhY2Vib29rMQNjb20AAAEAAQAAKQIAAACAAAAMAAoACOxjCAG9zVgzWgUDY29tAGAAbQAAAAByZLM4AAAAAQAAAAAAAQJoNQdleGFtcGxlA2NvbQAABgABAAApBNABAUAAADkADwA1AAlubyBTRVAgbWF0Y2hpbmcgdGhlIERTIGZvdW5kIGZvciBkbnNzZWMtZmFpbGVkLm9yZy54AQ==")],
            want: Ok({
                let timestamp = Value::Timestamp(
                    Utc.from_utc_datetime(
                        &DateTime::parse_from_rfc3339("2020-06-30T03:50:07.920014129Z")
                            .unwrap()
                            .naive_utc(),
                    ),
                );
                value!({
                    dataType: "Message",
                    dataTypeId: 1,
                    extraInfo: "",
                    messageType: "ResolverQuery",
                    messageTypeId: 3,
                    queryZone: "com.",
                    requestData: {
                        fullRcode: 0,
                        header: {
                            aa: false,
                            ad: false,
                            anCount: 0,
                            arCount: 1,
                            cd: false,
                            id: 37634,
                            nsCount: 0,
                            opcode: 0,
                            qdCount: 1,
                            qr: 0,
                            ra: false,
                            rcode: 0,
                            rd: false,
                            tc: false,
                        },
                        opt: {
                            do: true,
                            ednsVersion: 0,
                            extendedRcode: 0,
                            options: [
                            {
                                optCode: 10,
                                optName: "Cookie",
                                optValue: "7GMIAb3NWDM=",
                            }
                            ],
                            udpPayloadSize: 512,
                        },
                        question: [
                        {
                            class: "IN",
                            domainName: "facebook1.com.",
                            questionType: "A",
                            questionTypeId: 1,
                        }
                        ],
                        rcodeName: "NoError",
                    },
                    responseData: {
                        fullRcode: 16,
                        header: {
                            aa: false,
                            ad: false,
                            anCount: 0,
                            arCount: 1,
                            cd: false,
                            id: 45880,
                            nsCount: 0,
                            opcode: 0,
                            qdCount: 1,
                            qr: 0,
                            ra: false,
                            rcode: 16,
                            rd: false,
                            tc: false,
                        },
                        opt: {
                            do: false,
                            ede: [
                            {
                                extraText: "no SEP matching the DS found for dnssec-failed.org.",
                                infoCode: 9,
                                purpose: "DNSKEY Missing",
                            }
                            ],
                            ednsVersion: 1,
                            extendedRcode: 1,
                            udpPayloadSize: 1232,
                        },
                        question: [
                        {
                            class: "IN",
                            domainName: "h5.example.com.",
                            questionType: "SOA",
                            questionTypeId: 6,
                        }
                        ],
                        rcodeName: "BADSIG",
                    },
                    responseAddress: "2001:502:7094::30",
                    responsePort: 53,
                    serverId: "james-Virtual-Machine",
                    serverVersion: "BIND 9.16.3",
                    socketFamily: "INET6",
                    socketProtocol: "UDP",
                    sourceAddress: "::",
                    sourcePort: 46835,
                    time: 1_593_489_007_920_014_129i64,
                    timePrecision: "ns",
                    timestamp: timestamp
                })
            }),
            tdef: TypeDef::object(DnstapEventSchema.request_message_schema_definition()).fallible(),
        }

        update {
            args: func_args![value: value!("ChVqYW1lcy1WaXJ0dWFsLU1hY2hpbmUSC0JJTkQgOS4xNi4zcmsIDhABGAEiBH8AAAEqBH8AAAEwrG44AEC+iu73BU14gfofUh1wi6gAAAEAAAAAAAAHZXhhbXBsZQNjb20AAAYAAWC+iu73BW0agDwvch1wi6gAAAEAAAAAAAAHZXhhbXBsZQNjb20AAAYAAXgB")],
            want: Ok({
                let timestamp = Value::Timestamp(
                    Utc.from_utc_datetime(
                        &DateTime::parse_from_rfc3339("2020-06-30T18:32:30.792494106Z")
                            .unwrap()
                            .naive_utc(),
                    ),
                );
                value!({
                    dataType: "Message",
                    dataTypeId: 1,
                    messageType: "UpdateResponse",
                    messageTypeId: 14,
                    requestData: {
                        fullRcode: 0,
                        header: {
                            adCount: 0,
                            id: 28811,
                            opcode: 5,
                            prCount: 0,
                            qr: 1,
                            rcode: 0,
                            upCount: 0,
                            zoCount: 1
                        },
                        zone: {
                            zClass: "IN",
                            zName: "example.com.",
                            zType: "SOA",
                            zTypeId: 6
                        },
                        rcodeName: "NoError",
                    },
                    responseAddress: "127.0.0.1",
                    responseData: {
                        fullRcode: 0,
                        header: {
                            adCount: 0,
                            id: 28811,
                            opcode: 5,
                            prCount: 0,
                            qr: 1,
                            rcode: 0,
                            upCount: 0,
                            zoCount: 1
                        },
                        zone: {
                            zClass: "IN",
                            zName: "example.com.",
                            zType: "SOA",
                            zTypeId: 6
                        },
                        rcodeName: "NoError",
                    },
                    responsePort: 0,
                    serverId: "james-Virtual-Machine",
                    serverVersion: "BIND 9.16.3",
                    socketFamily: "INET",
                    socketProtocol: "UDP",
                    sourceAddress: "127.0.0.1",
                    sourcePort: 14124,
                    time: 1_593_541_950_792_494_106i64,
                    timePrecision: "ns",
                    timestamp: timestamp
                })
            }),
            tdef: TypeDef::object(DnstapEventSchema.request_message_schema_definition()).fallible(),
        }

        non_base64_value {
            args: func_args![value: value!("non base64 string")],
            want: Err("non base64 string is not a valid base64 encoded string"),
            tdef: TypeDef::object(DnstapEventSchema.request_message_schema_definition()).fallible(),
        }

        invalid_dnstap_data {
            args: func_args![value: value!("bm9uIGRuc3RhcCBkYXRh")],
            want: Err("dnstap parsing failed for bm9uIGRuc3RhcCBkYXRh: failed to decode Protobuf message: invalid wire type value: 6"),
            tdef: TypeDef::object(DnstapEventSchema.request_message_schema_definition()).fallible(),
        }
    ];
}
