use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, net::IpAddr};
use vrl::prelude::*;

// References:
// - https://docs.aws.amazon.com/awscloudtrail/latest/userguide/cloudtrail-event-reference-record-contents.html
// - https://github.com/aws/aws-cloudtrail-processing-library
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogs {
    #[serde(rename(deserialize = "Records"))]
    records: Vec<AwsCloudTrailLogsRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecord {
    event_time: DateTime<Utc>,
    event_version: semver::VersionReq,
    user_identity: AwsCloudTrailLogsRecordUserIdentity,
    event_source: String,
    event_name: String,
    aws_region: String,
    #[serde(rename(deserialize = "sourceIPAddress"))]
    source_ip_address: IpAddr,
    user_agent: String,
    error_code: Option<String>,
    error_message: Option<String>,
    request_parameters: serde_json::Map<String, serde_json::Value>,
    response_elements: serde_json::Value,
    additional_event_data: Option<String>,
    #[serde(rename(deserialize = "requestID"))]
    request_id: Option<String>,
    #[serde(rename(deserialize = "eventID"))]
    event_id: Option<String>,
    event_type: Option<AwsCloudTrailLogsRecordEventType>,
    api_version: Option<semver::VersionReq>,
    management_event: Option<AwsCloudTrailLogsRecordManagementEvent>,
    read_only: Option<bool>,
    resources: Option<Vec<AwsCloudTrailLogsRecordResources>>,
    recipient_account_id: Option<String>,
    service_event_details: Option<String>,
    #[serde(rename(deserialize = "sharedEventID"))]
    shared_event_id: Option<String>,
    vpc_endpoint_id: Option<String>,
    event_category: Option<AwsCloudTrailLogsRecordEventCategory>,
    addendum: Option<AwsCloudTrailLogsRecordAddendum>,
    session_credentials_from_console: Option<bool>,
    edge_device_details: Option<String>,
    tls_details: Option<AwsCloudTrailLogsRecordTlsDetails>,
    insight_details: Option<AwsCloudTrailLogsRecordInsightDetails>,
}

// Reference: https://docs.aws.amazon.com/awscloudtrail/latest/userguide/cloudtrail-event-reference-user-identity.html.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordUserIdentity {
    r#type: AwsCloudTrailLogsRecordUserIdentityType,
    user_name: Option<String>,
    principal_id: String,
    arn: String,
    account_id: String,
    access_key_id: String,
    session_context: Option<AwsCloudTrailLogsRecordUserIdentitySessionContext>,
    invoked_by: Option<String>,
    identity_provider: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum AwsCloudTrailLogsRecordUserIdentityType {
    Root,
    #[serde(rename(deserialize = "IAMUser"))]
    IamUser,
    AssumedRole,
    FederatedUser,
    #[serde(rename(deserialize = "AWSAccount"))]
    AwsAccount,
    #[serde(rename(deserialize = "AWSService"))]
    AwsService,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordUserIdentitySessionContext {
    session_issuer: Option<AwsCloudTrailLogsRecordUserIdentitySessionContextSessionIssuer>,
    web_id_federation_data:
        Option<AwsCloudTrailLogsRecordUserIdentitySessionContextWebIdFederationData>,
    attributes: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordUserIdentitySessionContextSessionIssuer {
    r#type: AwsCloudTrailLogsRecordUserIdentitySessionContextSessionIssuerType,
    user_name: String,
    principal_id: String,
    arn: String,
    account_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordUserIdentitySessionContextWebIdFederationData {
    federated_provider: String,
    attributes: Value,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum AwsCloudTrailLogsRecordUserIdentitySessionContextSessionIssuerType {
    Root,
    #[serde(rename(deserialize = "IAMUser"))]
    IamUser,
    Role,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[allow(clippy::enum_variant_names)] // Keep the `AWS` prefix
enum AwsCloudTrailLogsRecordEventType {
    AwsApiCall,
    AwsServiceEvent,
    #[serde(rename(deserialize = "AwsConsoleSignin"))]
    AwsConsoleSignIn,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[allow(clippy::enum_variant_names)] // Keep the `AWS` prefix
enum AwsCloudTrailLogsRecordManagementEvent {
    AwsApiCall,
    AwsConsoleAction,
    AwsConsoleSignIn,
    AwsServiceEvent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordResources {
    #[serde(rename(deserialize = "ARN"))]
    arn: String,
    account_id: String,
    r#type: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum AwsCloudTrailLogsRecordEventCategory {
    Management,
    Data,
    Insight,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordAddendum {
    reason: AwsCloudTrailLogsRecordAddendumReason,
    updated_fields: Option<String>,
    original_request_id: Option<String>,
    original_event_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "SCREAMING_SNAKE_CASE"))]
enum AwsCloudTrailLogsRecordAddendumReason {
    DeliveryDelay,
    UpdatedData,
    ServiceOutage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordTlsDetails {
    tls_version: String,
    cipher_suite: String,
    client_provided_host_header: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordInsightDetails {
    state: AwsCloudTrailLogsRecordInsightDetailsState,
    event_source: String,
    event_name: String,
    insight_type: AwsCloudTrailLogsRecordInsightDetailsInsightType,
    insight_context: AwsCloudTrailLogsRecordInsightDetailsInsightContext,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum AwsCloudTrailLogsRecordInsightDetailsState {
    Start,
    End,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum AwsCloudTrailLogsRecordInsightDetailsInsightType {
    ApiCallRateInsight,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordInsightDetailsInsightContext {
    statistics: AwsCloudTrailLogsRecordInsightDetailsInsightContextStatistics,
    attributions: Option<Vec<AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributions>>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordInsightDetailsInsightContextStatistics {
    baseline: AwsCloudTrailLogsRecordInsightDetailsInsightContextStatisticsValue,
    insight: AwsCloudTrailLogsRecordInsightDetailsInsightContextStatisticsValue,
    insight_duration: u64,
    baseline_duration: u64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordInsightDetailsInsightContextStatisticsValue {
    average: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributions {
    attribute: AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsAttribute,
    insight: Vec<AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsInsight>,
    baseline: Vec<AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsBaseline>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
enum AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsAttribute {
    UserIdentityArn,
    UserAgent,
    ErrorCode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsInsight {
    value: String,
    average: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsBaseline {
    value: String,
    average: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct ParseAwsCloudTrailLogs;

impl Function for ParseAwsCloudTrailLogs {
    fn identifier(&self) -> &'static str {
        "parse_aws_cloudtrail_logs"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "valid",
            source: indoc! {r#"
                parse_aws_cloudtrail_logs!(s'{
                    "Records": [{
                        "eventVersion": "1.0",
                        "userIdentity": {
                            "type": "IAMUser",
                            "principalId": "EX_PRINCIPAL_ID",
                            "arn": "arn:aws:iam::123456789012:user/Alice",
                            "accessKeyId": "EXAMPLE_KEY_ID",
                            "accountId": "123456789012",
                            "userName": "Alice"
                        },
                        "eventTime": "2014-03-06T21:22:54Z",
                        "eventSource": "ec2.amazonaws.com",
                        "eventName": "StartInstances",
                        "awsRegion": "us-east-2",
                        "sourceIPAddress": "205.251.233.176",
                        "userAgent": "ec2-api-tools 1.6.12.2",
                        "requestParameters": {
                            "instancesSet": {
                                "items": [{
                                    "instanceId": "i-ebeaf9e2"
                                }]
                            }
                        },
                        "responseElements": {
                            "instancesSet": {
                                "items": [{
                                    "instanceId": "i-ebeaf9e2",
                                    "currentState": {
                                        "code": 0,
                                        "name": "pending"
                                    },
                                    "previousState": {
                                        "code": 80,
                                        "name": "stopped"
                                    }
                                }]
                            }
                        }
                    }]
                }')
            "#},
            result: Ok(indoc! {r#"{}"#}),
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ParseAwsCloudTrailLogsFn { value }))
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
struct ParseAwsCloudTrailLogsFn {
    value: Box<dyn Expression>,
}

impl Expression for ParseAwsCloudTrailLogsFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?.try_bytes()?;

        // TODO
        let _value = serde_json::from_slice::<AwsCloudTrailLogs>(&bytes)
            .map_err(|error| format!("unable to parse AWS CloudTrail logs: {}", error))?;

        Ok(Value::Array(vec![]))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new()
            .fallible() // Message parsing error
            .array_mapped::<(), TypeDef>(map! { (): TypeDef::new().object(inner_type_def()) })
    }
}

fn inner_type_def() -> BTreeMap<&'static str, TypeDef> {
    map! {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cloudtrail_user_identity_with_iam_user_credentials() {
        let json = indoc! {r#"
            {
                "type": "IAMUser",
                "principalId": "AIDAJ45Q7YFFAREXAMPLE",
                "arn": "arn:aws:iam::123456789012:user/Alice",
                "accountId": "123456789012",
                "accessKeyId": "",
                "userName": "Alice"
            }
        "#};

        serde_json::from_str::<AwsCloudTrailLogsRecordUserIdentity>(json)
            .expect("parses successfully");
    }

    #[test]
    fn test_parse_cloudtrail_user_identity_with_temporary_security_credentials() {
        let json = indoc! {r#"
            {
                "type": "AssumedRole",
                "principalId": "AROAIDPPEZS35WEXAMPLE:AssumedRoleSessionName",
                "arn": "arn:aws:sts::123456789012:assumed-role/RoleToBeAssumed/MySessionName",
                "accountId": "123456789012",
                "accessKeyId": "",
                "sessionContext": {
                    "attributes": {
                        "mfaAuthenticated": "false",
                        "creationDate": "20131102T010628Z"
                    },
                    "sessionIssuer": {
                        "type": "Role",
                        "principalId": "AROAIDPPEZS35WEXAMPLE",
                        "arn": "arn:aws:iam::123456789012:role/RoleToBeAssumed",
                        "accountId": "123456789012",
                        "userName": "RoleToBeAssumed"
                    }
                }
            }
        "#};

        serde_json::from_str::<AwsCloudTrailLogsRecordUserIdentity>(json)
            .expect("parses successfully");
    }

    #[test]
    fn test_parse_cloudtrail_insight_details() {
        let json = indoc! {r#"
            {
                "state": "Start",
                "eventSource": "autoscaling.amazonaws.com",
                "eventName": "CompleteLifecycleAction",
                "insightType": "ApiCallRateInsight",
                "insightContext": {
                    "statistics": {
                        "baseline": {
                            "average": 0.0000882145
                        },
                        "insight": {
                            "average": 0.6
                        },
                        "insightDuration": 5,
                        "baselineDuration": 11336
                    },
                    "attributions": [
                        {
                            "attribute": "userIdentityArn",
                            "insight": [
                                {
                                    "value": "arn:aws:sts::012345678901:assumed-role/CodeDeployRole1",
                                    "average": 0.2
                                },
                                {
                                    "value": "arn:aws:sts::012345678901:assumed-role/CodeDeployRole2",
                                    "average": 0.2
                                },
                                {
                                    "value": "arn:aws:sts::012345678901:assumed-role/CodeDeployRole3",
                                    "average": 0.2
                                }
                            ],
                            "baseline": [
                                {
                                    "value": "arn:aws:sts::012345678901:assumed-role/CodeDeployRole1",
                                    "average": 0.0000882145
                                }
                            ]
                        },
                        {
                            "attribute": "userAgent",
                            "insight": [
                                {
                                    "value": "codedeploy.amazonaws.com",
                                    "average": 0.6
                                }
                            ],
                            "baseline": [
                                {
                                    "value": "codedeploy.amazonaws.com",
                                    "average": 0.0000882145
                                }
                            ]
                        },
                        {
                            "attribute": "errorCode",
                            "insight": [
                                {
                                    "value": "null",
                                    "average": 0.6
                                }
                            ],
                            "baseline": [
                                {
                                    "value": "null",
                                    "average": 0.0000882145
                                }
                            ]
                        }
                    ]
                }
            }
        "#};

        serde_json::from_str::<AwsCloudTrailLogsRecordInsightDetails>(json)
            .expect("parses successfully");
    }

    #[test]
    fn test_parse_cloudtrail_ec2_logs() {
        let json = indoc! {r#"
            {
                "Records": [
                    {
                        "eventVersion": "1.0",
                        "userIdentity": {
                            "type": "IAMUser",
                            "principalId": "EX_PRINCIPAL_ID",
                            "arn": "arn:aws:iam::123456789012:user/Alice",
                            "accessKeyId": "EXAMPLE_KEY_ID",
                            "accountId": "123456789012",
                            "userName": "Alice"
                        },
                        "eventTime": "2014-03-06T21:22:54Z",
                        "eventSource": "ec2.amazonaws.com",
                        "eventName": "StartInstances",
                        "awsRegion": "us-east-2",
                        "sourceIPAddress": "205.251.233.176",
                        "userAgent": "ec2-api-tools 1.6.12.2",
                        "requestParameters": {
                            "instancesSet": {
                                "items": [
                                    {
                                        "instanceId": "i-ebeaf9e2"
                                    }
                                ]
                            }
                        },
                        "responseElements": {
                            "instancesSet": {
                                "items": [
                                    {
                                        "instanceId": "i-ebeaf9e2",
                                        "currentState": {
                                            "code": 0,
                                            "name": "pending"
                                        },
                                        "previousState": {
                                            "code": 80,
                                            "name": "stopped"
                                        }
                                    }
                                ]
                            }
                        }
                    }
                ]
            }
        "#};

        serde_json::from_str::<AwsCloudTrailLogs>(json).expect("parses successfully");
    }

    test_function![
        parse_aws_cloudtrail_logs => ParseAwsCloudTrailLogs;

        string {
            args: func_args![value: indoc!{ r#"
                {
                    "Records": [
                        {
                            "eventVersion": "1.0",
                            "userIdentity": {
                                "type": "IAMUser",
                                "principalId": "EX_PRINCIPAL_ID",
                                "arn": "arn:aws:iam::123456789012:user/Alice",
                                "accessKeyId": "EXAMPLE_KEY_ID",
                                "accountId": "123456789012",
                                "userName": "Alice"
                            },
                            "eventTime": "2014-03-06T21:22:54Z",
                            "eventSource": "ec2.amazonaws.com",
                            "eventName": "StartInstances",
                            "awsRegion": "us-east-2",
                            "sourceIPAddress": "205.251.233.176",
                            "userAgent": "ec2-api-tools 1.6.12.2",
                            "requestParameters": {
                                "instancesSet": {
                                    "items": [
                                        {
                                            "instanceId": "i-ebeaf9e2"
                                        }
                                    ]
                                }
                            },
                            "responseElements": {
                                "instancesSet": {
                                    "items": [
                                        {
                                            "instanceId": "i-ebeaf9e2",
                                            "currentState": {
                                                "code": 0,
                                                "name": "pending"
                                            },
                                            "previousState": {
                                                "code": 80,
                                                "name": "stopped"
                                            }
                                        }
                                    ]
                                }
                            }
                        }
                    ]
                }
            "#}],
            want: Ok(Value::Array(vec![])),
            tdef: TypeDef::new().fallible().array_mapped::<(), TypeDef>(map! { (): TypeDef::new().object(inner_type_def()) }),
        }

        invalid_value {
            args: func_args![value: r#"{ INVALID }"#],
            want: Err("unable to parse AWS CloudTrail logs: key must be a string at line 1 column 3"),
            tdef: TypeDef::new().fallible().array_mapped::<(), TypeDef>(map! { (): TypeDef::new().object(inner_type_def()) }),
        }

        invalid_type {
            args: func_args![value: "42"],
            want: Err("unable to parse AWS CloudTrail logs: invalid type: integer `42`, expected struct AwsCloudTrailLogs at line 1 column 2"),
            tdef: TypeDef::new().fallible().array_mapped::<(), TypeDef>(map! { (): TypeDef::new().object(inner_type_def()) }),
        }
    ];
}
