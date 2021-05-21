use chrono::{DateTime, Utc};
use derive_more::Display;
use serde::Deserialize;
use std::{collections::BTreeMap, net::IpAddr};
use vrl::prelude::*;

// References:
// - https://docs.aws.amazon.com/awscloudtrail/latest/userguide/cloudtrail-event-reference-record-contents.html
// - https://github.com/aws/aws-cloudtrail-processing-library
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogs {
    #[serde(rename(deserialize = "Records"))]
    records: Vec<AwsCloudTrailLogsRecord>,
}

impl From<AwsCloudTrailLogs> for Value {
    fn from(logs: AwsCloudTrailLogs) -> Self {
        logs.records.into()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecord {
    event_time: DateTime<Utc>,
    event_version: String,
    user_identity: AwsCloudTrailLogsRecordUserIdentity,
    event_source: String,
    event_name: String,
    aws_region: String,
    #[serde(rename(deserialize = "sourceIPAddress"))]
    source_ip_address: IpAddr,
    user_agent: String,
    error_code: Option<String>,
    error_message: Option<String>,
    request_parameters: BTreeMap<String, Value>,
    response_elements: BTreeMap<String, Value>,
    additional_event_data: Option<String>,
    #[serde(rename(deserialize = "requestID"))]
    request_id: Option<String>,
    #[serde(rename(deserialize = "eventID"))]
    event_id: Option<String>,
    event_type: Option<AwsCloudTrailLogsRecordEventType>,
    api_version: Option<String>,
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

impl From<AwsCloudTrailLogsRecord> for Value {
    fn from(record: AwsCloudTrailLogsRecord) -> Self {
        let mut value = BTreeMap::<String, Value>::new();

        value.insert("event_time".to_owned(), record.event_time.into());
        value.insert(
            "event_version".to_owned(),
            record.event_version.to_string().into(),
        );
        value.insert("user_identity".to_owned(), record.user_identity.into());
        value.insert("event_source".to_owned(), record.event_source.into());
        value.insert("event_name".to_owned(), record.event_name.into());
        value.insert("aws_region".to_owned(), record.aws_region.into());
        value.insert(
            "source_ip_address".to_owned(),
            record.source_ip_address.to_string().into(),
        );
        value.insert("user_agent".to_owned(), record.user_agent.into());
        if let Some(error_code) = record.error_code {
            value.insert("error_code".to_owned(), error_code.into());
        }
        if let Some(error_message) = record.error_message {
            value.insert("error_message".to_owned(), error_message.into());
        }
        value.insert(
            "request_parameters".to_owned(),
            record.request_parameters.into(),
        );
        value.insert(
            "response_elements".to_owned(),
            record.response_elements.into(),
        );
        if let Some(additional_event_data) = record.additional_event_data {
            value.insert(
                "additional_event_data".to_owned(),
                additional_event_data.into(),
            );
        }
        if let Some(request_id) = record.request_id {
            value.insert("request_id".to_owned(), request_id.into());
        }
        if let Some(event_id) = record.event_id {
            value.insert("event_id".to_owned(), event_id.into());
        }
        if let Some(event_type) = record.event_type {
            value.insert("event_type".to_owned(), event_type.into());
        }
        if let Some(api_version) = record.api_version {
            value.insert("api_version".to_owned(), api_version.into());
        }
        if let Some(management_event) = record.management_event {
            value.insert("management_event".to_owned(), management_event.into());
        }
        if let Some(read_only) = record.read_only {
            value.insert("read_only".to_owned(), read_only.into());
        }
        if let Some(resources) = record.resources {
            value.insert("resources".to_owned(), resources.into());
        }
        if let Some(recipient_account_id) = record.recipient_account_id {
            value.insert(
                "recipient_account_id".to_owned(),
                recipient_account_id.into(),
            );
        }
        if let Some(service_event_details) = record.service_event_details {
            value.insert(
                "service_event_details".to_owned(),
                service_event_details.into(),
            );
        }
        if let Some(shared_event_id) = record.shared_event_id {
            value.insert("shared_event_id".to_owned(), shared_event_id.into());
        }
        if let Some(vpc_endpoint_id) = record.vpc_endpoint_id {
            value.insert("vpc_endpoint_id".to_owned(), vpc_endpoint_id.into());
        }
        if let Some(event_category) = record.event_category {
            value.insert("event_category".to_owned(), event_category.into());
        }
        if let Some(addendum) = record.addendum {
            value.insert("addendum".to_owned(), addendum.into());
        }
        if let Some(session_credentials_from_console) = record.session_credentials_from_console {
            value.insert(
                "session_credentials_from_console".to_owned(),
                session_credentials_from_console.into(),
            );
        }
        if let Some(edge_device_details) = record.edge_device_details {
            value.insert("edge_device_details".to_owned(), edge_device_details.into());
        }
        if let Some(tls_details) = record.tls_details {
            value.insert("tls_details".to_owned(), tls_details.into());
        }
        if let Some(insight_details) = record.insight_details {
            value.insert("insight_details".to_owned(), insight_details.into());
        }

        value.into()
    }
}

// Reference: https://docs.aws.amazon.com/awscloudtrail/latest/userguide/cloudtrail-event-reference-user-identity.html.
#[derive(Clone, Debug, Deserialize)]
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

impl From<AwsCloudTrailLogsRecordUserIdentity> for Value {
    fn from(user_identity: AwsCloudTrailLogsRecordUserIdentity) -> Self {
        let mut value = BTreeMap::<String, Value>::new();

        value.insert("type".to_owned(), user_identity.r#type.into());
        if let Some(user_name) = user_identity.user_name {
            value.insert("user_name".to_owned(), user_name.into());
        }
        value.insert("principal_id".to_owned(), user_identity.principal_id.into());
        value.insert("arn".to_owned(), user_identity.arn.into());
        value.insert("account_id".to_owned(), user_identity.account_id.into());
        value.insert(
            "access_key_id".to_owned(),
            user_identity.access_key_id.into(),
        );
        if let Some(session_context) = user_identity.session_context {
            value.insert("session_context".to_owned(), session_context.into());
        }
        if let Some(invoked_by) = user_identity.invoked_by {
            value.insert("invoked_by".to_owned(), invoked_by.into());
        }
        if let Some(identity_provider) = user_identity.identity_provider {
            value.insert("identity_provider".to_owned(), identity_provider.into());
        }

        value.into()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Display)]
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

impl From<AwsCloudTrailLogsRecordUserIdentityType> for Value {
    fn from(identity_type: AwsCloudTrailLogsRecordUserIdentityType) -> Self {
        format!("{}", identity_type).into()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordUserIdentitySessionContext {
    session_issuer: Option<AwsCloudTrailLogsRecordUserIdentitySessionContextSessionIssuer>,
    web_id_federation_data:
        Option<AwsCloudTrailLogsRecordUserIdentitySessionContextWebIdFederationData>,
    attributes: Option<BTreeMap<String, Value>>,
}

impl From<AwsCloudTrailLogsRecordUserIdentitySessionContext> for Value {
    fn from(session_context: AwsCloudTrailLogsRecordUserIdentitySessionContext) -> Self {
        let mut value = BTreeMap::<String, Value>::new();

        if let Some(session_issuer) = session_context.session_issuer {
            value.insert("session_issuer".to_owned(), session_issuer.into());
        }
        if let Some(web_id_federation_data) = session_context.web_id_federation_data {
            value.insert(
                "web_id_federation_data".to_owned(),
                web_id_federation_data.into(),
            );
        }
        if let Some(attributes) = session_context.attributes {
            value.insert("attributes".to_owned(), attributes.into());
        }

        value.into()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordUserIdentitySessionContextSessionIssuer {
    r#type: AwsCloudTrailLogsRecordUserIdentitySessionContextSessionIssuerType,
    user_name: String,
    principal_id: String,
    arn: String,
    account_id: String,
}

impl From<AwsCloudTrailLogsRecordUserIdentitySessionContextSessionIssuer> for Value {
    fn from(
        session_issuer: AwsCloudTrailLogsRecordUserIdentitySessionContextSessionIssuer,
    ) -> Self {
        let mut value = BTreeMap::<String, Value>::new();

        value.insert("type".to_owned(), session_issuer.r#type.into());
        value.insert("user_name".to_owned(), session_issuer.user_name.into());
        value.insert(
            "principal_id".to_owned(),
            session_issuer.principal_id.into(),
        );
        value.insert("arn".to_owned(), session_issuer.arn.into());
        value.insert("account_id".to_owned(), session_issuer.account_id.into());

        value.into()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordUserIdentitySessionContextWebIdFederationData {
    federated_provider: String,
    attributes: BTreeMap<String, Value>,
}

impl From<AwsCloudTrailLogsRecordUserIdentitySessionContextWebIdFederationData> for Value {
    fn from(
        federation_data: AwsCloudTrailLogsRecordUserIdentitySessionContextWebIdFederationData,
    ) -> Self {
        let mut value = BTreeMap::<String, Value>::new();

        value.insert(
            "federated_provider".to_owned(),
            federation_data.federated_provider.into(),
        );
        value.insert("attributes".to_owned(), federation_data.attributes.into());

        value.into()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Display)]
enum AwsCloudTrailLogsRecordUserIdentitySessionContextSessionIssuerType {
    Root,
    #[serde(rename(deserialize = "IAMUser"))]
    IamUser,
    Role,
}

impl From<AwsCloudTrailLogsRecordUserIdentitySessionContextSessionIssuerType> for Value {
    fn from(
        issuer_type: AwsCloudTrailLogsRecordUserIdentitySessionContextSessionIssuerType,
    ) -> Self {
        format!("{}", issuer_type).into()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Display)]
#[allow(clippy::enum_variant_names)] // Keep the `AWS` prefix
enum AwsCloudTrailLogsRecordEventType {
    AwsApiCall,
    AwsServiceEvent,
    #[serde(rename(deserialize = "AwsConsoleSignin"))]
    AwsConsoleSignIn,
}

impl From<AwsCloudTrailLogsRecordEventType> for Value {
    fn from(event_type: AwsCloudTrailLogsRecordEventType) -> Self {
        format!("{}", event_type).into()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Display)]
#[allow(clippy::enum_variant_names)] // Keep the `AWS` prefix
enum AwsCloudTrailLogsRecordManagementEvent {
    AwsApiCall,
    AwsConsoleAction,
    AwsConsoleSignIn,
    AwsServiceEvent,
}

impl From<AwsCloudTrailLogsRecordManagementEvent> for Value {
    fn from(management_event: AwsCloudTrailLogsRecordManagementEvent) -> Self {
        format!("{}", management_event).into()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordResources {
    #[serde(rename(deserialize = "ARN"))]
    arn: String,
    account_id: String,
    r#type: String,
}

impl From<AwsCloudTrailLogsRecordResources> for Value {
    fn from(resources: AwsCloudTrailLogsRecordResources) -> Self {
        let mut value = BTreeMap::<String, Value>::new();

        value.insert("arn".to_owned(), resources.arn.into());
        value.insert("account_id".to_owned(), resources.account_id.into());
        value.insert("type".to_owned(), resources.r#type.into());

        value.into()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Display)]
enum AwsCloudTrailLogsRecordEventCategory {
    Management,
    Data,
    Insight,
}

impl From<AwsCloudTrailLogsRecordEventCategory> for Value {
    fn from(event_category: AwsCloudTrailLogsRecordEventCategory) -> Self {
        format!("{}", event_category).into()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordAddendum {
    reason: AwsCloudTrailLogsRecordAddendumReason,
    updated_fields: Option<String>,
    original_request_id: Option<String>,
    original_event_id: Option<String>,
}

impl From<AwsCloudTrailLogsRecordAddendum> for Value {
    fn from(addendum: AwsCloudTrailLogsRecordAddendum) -> Self {
        let mut value = BTreeMap::<String, Value>::new();

        value.insert("reason".to_owned(), addendum.reason.into());
        if let Some(updated_fields) = addendum.updated_fields {
            value.insert("updated_fields".to_owned(), updated_fields.into());
        }
        if let Some(original_request_id) = addendum.original_request_id {
            value.insert("original_request_id".to_owned(), original_request_id.into());
        }
        if let Some(original_event_id) = addendum.original_event_id {
            value.insert("original_event_id".to_owned(), original_event_id.into());
        }

        value.into()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Display)]
#[serde(rename_all(deserialize = "SCREAMING_SNAKE_CASE"))]
enum AwsCloudTrailLogsRecordAddendumReason {
    DeliveryDelay,
    UpdatedData,
    ServiceOutage,
}

impl From<AwsCloudTrailLogsRecordAddendumReason> for Value {
    fn from(addendum_reason: AwsCloudTrailLogsRecordAddendumReason) -> Self {
        format!("{}", addendum_reason).into()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordTlsDetails {
    tls_version: String,
    cipher_suite: String,
    client_provided_host_header: String,
}

impl From<AwsCloudTrailLogsRecordTlsDetails> for Value {
    fn from(tls_details: AwsCloudTrailLogsRecordTlsDetails) -> Self {
        let mut value = BTreeMap::<String, Value>::new();

        value.insert("tls_version".to_owned(), tls_details.tls_version.into());
        value.insert("cipher_suite".to_owned(), tls_details.cipher_suite.into());
        value.insert(
            "client_provided_host_header".to_owned(),
            tls_details.client_provided_host_header.into(),
        );

        value.into()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordInsightDetails {
    state: AwsCloudTrailLogsRecordInsightDetailsState,
    event_source: String,
    event_name: String,
    insight_type: AwsCloudTrailLogsRecordInsightDetailsInsightType,
    insight_context: AwsCloudTrailLogsRecordInsightDetailsInsightContext,
}

impl From<AwsCloudTrailLogsRecordInsightDetails> for Value {
    fn from(insight_details: AwsCloudTrailLogsRecordInsightDetails) -> Self {
        let mut value = BTreeMap::<String, Value>::new();

        value.insert("state".to_owned(), insight_details.state.into());
        value.insert(
            "event_source".to_owned(),
            insight_details.event_source.into(),
        );
        value.insert("event_name".to_owned(), insight_details.event_name.into());
        value.insert(
            "insight_type".to_owned(),
            insight_details.insight_type.into(),
        );
        value.insert(
            "insight_context".to_owned(),
            insight_details.insight_context.into(),
        );

        value.into()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Display)]
enum AwsCloudTrailLogsRecordInsightDetailsState {
    Start,
    End,
}

impl From<AwsCloudTrailLogsRecordInsightDetailsState> for Value {
    fn from(details_state: AwsCloudTrailLogsRecordInsightDetailsState) -> Self {
        format!("{}", details_state).into()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Display)]
enum AwsCloudTrailLogsRecordInsightDetailsInsightType {
    ApiCallRateInsight,
}

impl From<AwsCloudTrailLogsRecordInsightDetailsInsightType> for Value {
    fn from(insight_type: AwsCloudTrailLogsRecordInsightDetailsInsightType) -> Self {
        format!("{}", insight_type).into()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordInsightDetailsInsightContext {
    statistics: AwsCloudTrailLogsRecordInsightDetailsInsightContextStatistics,
    attributions: Option<Vec<AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributions>>,
}

impl From<AwsCloudTrailLogsRecordInsightDetailsInsightContext> for Value {
    fn from(insight_context: AwsCloudTrailLogsRecordInsightDetailsInsightContext) -> Self {
        let mut value = BTreeMap::<String, Value>::new();

        value.insert("statistics".to_owned(), insight_context.statistics.into());
        if let Some(attributions) = insight_context.attributions {
            value.insert("attributions".to_owned(), attributions.into());
        }

        value.into()
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordInsightDetailsInsightContextStatistics {
    baseline: AwsCloudTrailLogsRecordInsightDetailsInsightContextStatisticsValue,
    insight: AwsCloudTrailLogsRecordInsightDetailsInsightContextStatisticsValue,
    insight_duration: u64,
    baseline_duration: u64,
}

impl From<AwsCloudTrailLogsRecordInsightDetailsInsightContextStatistics> for Value {
    fn from(statistics: AwsCloudTrailLogsRecordInsightDetailsInsightContextStatistics) -> Self {
        let mut value = BTreeMap::<String, Value>::new();

        value.insert("baseline".to_owned(), statistics.baseline.into());
        value.insert("insight".to_owned(), statistics.insight.into());
        value.insert(
            "insight_duration".to_owned(),
            statistics.insight_duration.into(),
        );
        value.insert(
            "baseline_duration".to_owned(),
            statistics.baseline_duration.into(),
        );

        value.into()
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordInsightDetailsInsightContextStatisticsValue {
    average: f64,
}

impl From<AwsCloudTrailLogsRecordInsightDetailsInsightContextStatisticsValue> for Value {
    fn from(
        statistics: AwsCloudTrailLogsRecordInsightDetailsInsightContextStatisticsValue,
    ) -> Self {
        let mut value = BTreeMap::<String, Value>::new();

        value.insert("average".to_owned(), statistics.average.into());

        value.into()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributions {
    attribute: AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsAttribute,
    insight: Vec<AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsInsight>,
    baseline: Vec<AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsBaseline>,
}

impl From<AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributions> for Value {
    fn from(attributions: AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributions) -> Self {
        let mut value = BTreeMap::<String, Value>::new();

        value.insert("attribute".to_owned(), attributions.attribute.into());
        value.insert("insight".to_owned(), attributions.insight.into());
        value.insert("baseline".to_owned(), attributions.baseline.into());

        value.into()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Display)]
#[serde(rename_all(deserialize = "camelCase"))]
enum AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsAttribute {
    UserIdentityArn,
    UserAgent,
    ErrorCode,
}

impl From<AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsAttribute> for Value {
    fn from(
        attribute: AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsAttribute,
    ) -> Self {
        format!("{}", attribute).into()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsInsight {
    value: String,
    average: f64,
}

impl From<AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsInsight> for Value {
    fn from(
        insight: AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsInsight,
    ) -> Self {
        let mut value = BTreeMap::<String, Value>::new();

        value.insert("value".to_owned(), insight.value.into());
        value.insert("average".to_owned(), insight.average.into());

        value.into()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
struct AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsBaseline {
    value: String,
    average: f64,
}

impl From<AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsBaseline> for Value {
    fn from(
        baseline: AwsCloudTrailLogsRecordInsightDetailsInsightContextAttributionsBaseline,
    ) -> Self {
        let mut value = BTreeMap::<String, Value>::new();

        value.insert("value".to_owned(), baseline.value.into());
        value.insert("average".to_owned(), baseline.average.into());

        value.into()
    }
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
            result: Ok(indoc! {r#"[{
                "aws_region": "us-east-2",
                "event_name": "StartInstances",
                "event_source": "ec2.amazonaws.com",
                "event_time": "2014-03-06T21:22:54Z",
                "event_version": "1.0",
                "request_parameters": {
                    "instancesSet": {
                        "items": [
                            {
                                "instanceId": "i-ebeaf9e2"
                            }
                        ]
                    }
                },
                "response_elements": {
                    "instancesSet": {
                        "items": [
                            {
                                "currentState": {
                                    "code": 0,
                                    "name": "pending"
                                },
                                "instanceId": "i-ebeaf9e2",
                                "previousState": {
                                    "code": 80,
                                    "name": "stopped"
                                }
                            }
                        ]
                    }
                },
                "source_ip_address": "205.251.233.176",
                "user_agent": "ec2-api-tools 1.6.12.2",
                "user_identity": {
                    "access_key_id": "EXAMPLE_KEY_ID",
                    "account_id": "123456789012",
                    "arn": "arn:aws:iam::123456789012:user/Alice",
                    "principal_id": "EX_PRINCIPAL_ID",
                    "type": "IamUser",
                    "user_name": "Alice"
                }
            }]"#}),
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

        let logs = serde_json::from_slice::<AwsCloudTrailLogs>(&bytes)
            .map_err(|error| format!("unable to parse AWS CloudTrail logs: {}", error))?;

        Ok(logs.into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new()
            .fallible() // Message parsing error
            .array_mapped::<(), TypeDef>(map! { (): TypeDef::new().object(inner_type_def()) })
    }
}

fn inner_type_def() -> BTreeMap<&'static str, TypeDef> {
    map! {
        "event_time": Kind::Timestamp,
        "event_version": Kind::Bytes,
        "user_identity": TypeDef::new().object::<&str, TypeDef>(map! {
            "type": Kind::Bytes,
            "user_name": Kind::Bytes | Kind::Null,
            "principal_id": Kind::Bytes,
            "arn": Kind::Bytes,
            "account_id": Kind::Bytes,
            "access_key_id": Kind::Bytes,
            "session_context": TypeDef::new().object::<&str, TypeDef>(map! {
                "session_issuer": TypeDef::new().object::<&str, TypeDef>(map! {
                    "type": Kind::Bytes,
                    "user_name": Kind::Bytes,
                    "principal_id": Kind::Bytes,
                    "arn": Kind::Bytes,
                    "account_id": Kind::Bytes,
                }).add_null(),
                "web_id_federation_data": TypeDef::new().object::<&str, TypeDef>(map! {
                    "federated_provider": Kind::Bytes,
                    "attributes": TypeDef::new().object::<(), Kind>(map! {
                        (): Kind::all()
                    })
                }).add_null(),
                "attributes": TypeDef::new().object::<(), Kind>(map! {
                    (): Kind::all()
                }),
            }).add_null(),
            "invoked_by": Kind::Bytes | Kind::Null,
            "identity_provider": Kind::Bytes | Kind::Null,
        }),
        "event_source": Kind::Bytes,
        "event_name": Kind::Bytes,
        "aws_region": Kind::Bytes,
        "source_ip_address": Kind::Bytes,
        "user_agent": Kind::Bytes,
        "error_code": Kind::Bytes | Kind::Null,
        "error_message": Kind::Bytes | Kind::Null,
        "request_parameters": TypeDef::new().object::<(), Kind>(map! {
            (): Kind::all()
        }),
        "response_elements": TypeDef::new().object::<(), Kind>(map! {
            (): Kind::all()
        }),
        "additional_event_data": Kind::Bytes | Kind::Null,
        "request_id": Kind::Bytes | Kind::Null,
        "event_id": Kind::Bytes | Kind::Null,
        "event_type": Kind::Bytes | Kind::Null,
        "api_version": Kind::Bytes | Kind::Null,
        "management_event": Kind::Bytes | Kind::Null,
        "read_only": Kind::Boolean | Kind::Null,
        "resources": TypeDef::new().array_mapped::<(), TypeDef>(map! { (): TypeDef::new().object::<&str, TypeDef>(map! {
            "arn": Kind::Bytes,
            "account_id": Kind::Bytes,
            "type": Kind::Bytes,
        })}).add_null(),
        "recipient_account_id": Kind::Bytes | Kind::Null,
        "service_event_details": Kind::Bytes | Kind::Null,
        "shared_event_id": Kind::Bytes | Kind::Null,
        "vpc_endpoint_id": Kind::Bytes | Kind::Null,
        "event_category": Kind::Bytes | Kind::Null,
        "addendum": TypeDef::new().object::<&str, TypeDef>(map! {
            "reason": Kind::Bytes,
            "updated_fields": Kind::Bytes | Kind::Null,
            "original_request_id": Kind::Bytes | Kind::Null,
            "original_event_id": Kind::Bytes | Kind::Null,
        }).add_null(),
        "session_credentials_from_console": Kind::Boolean | Kind::Null,
        "edge_device_details": Kind::Bytes | Kind::Null,
        "tls_details": TypeDef::new().object::<&str, TypeDef>(map! {
            "tls_version": Kind::Bytes,
            "cipher_suite": Kind::Bytes,
            "client_provided_host_header": Kind::Bytes,
        }).add_null(),
        "insight_details": TypeDef::new().object::<&str, TypeDef>(map! {
            "state": Kind::Bytes,
            "event_source": Kind::Bytes,
            "event_name": Kind::Bytes,
            "insight_type": Kind::Bytes,
            "insight_context": TypeDef::new().object::<&str, TypeDef>(map! {
                "statistics": TypeDef::new().object::<&str, TypeDef>(map! {
                    "baseline": TypeDef::new().object::<&str, TypeDef>(map! {
                        "average": Kind::Float,
                    }),
                    "insight": TypeDef::new().object::<&str, TypeDef>(map! {
                        "average": Kind::Float,
                    }),
                    "insight_duration": Kind::Integer,
                    "baseline_duration": Kind::Integer,
                }),
                "attributions": TypeDef::new().array_mapped::<(), TypeDef>(map! { (): TypeDef::new().object::<&str, TypeDef>(map! {
                    "attribute": Kind::Bytes,
                    "insight": TypeDef::new().array_mapped::<(), TypeDef>(map! { (): TypeDef::new().object::<&str, TypeDef>(map! {
                        "value": Kind::Bytes,
                        "average": Kind::Float,
                    })}),
                    "baseline": TypeDef::new().array_mapped::<(), TypeDef>(map! { (): TypeDef::new().object::<&str, TypeDef>(map! {
                        "value": Kind::Bytes,
                        "average": Kind::Float,
                    })}),
                })}).add_null(),
            }),
        }).add_null(),
    }
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
            want: Ok(vec![
                map! {
                    "event_version": "1.0",
                    "user_identity": map! {
                        "type": "IamUser",
                        "principal_id": "EX_PRINCIPAL_ID",
                        "arn": "arn:aws:iam::123456789012:user/Alice",
                        "access_key_id": "EXAMPLE_KEY_ID",
                        "account_id": "123456789012",
                        "user_name": "Alice"
                    },
                    "event_time": DateTime::parse_from_rfc3339("2014-03-06T21:22:54Z")
                        .unwrap()
                        .with_timezone(&Utc),
                    "event_source": "ec2.amazonaws.com",
                    "event_name": "StartInstances",
                    "aws_region": "us-east-2",
                    "source_ip_address": "205.251.233.176",
                    "user_agent": "ec2-api-tools 1.6.12.2",
                    "request_parameters": map! {
                        "instancesSet": map! {
                            "items": vec![
                                map! {
                                    "instanceId": "i-ebeaf9e2"
                                }
                            ]
                        }
                    },
                    "response_elements": map! {
                        "instancesSet": map! {
                            "items": vec![
                                map! {
                                    "instanceId": "i-ebeaf9e2",
                                    "currentState": map! {
                                        "code": 0,
                                        "name": "pending"
                                    },
                                    "previousState": map! {
                                        "code": 80,
                                        "name": "stopped"
                                    }
                                }
                            ]
                        }
                    }
                }
            ]),
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
