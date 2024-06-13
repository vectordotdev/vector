use std::convert::TryFrom;

use snafu::{ResultExt, Snafu};

use vector_lib::configurable::configurable_component;

use crate::{
    aws::AwsAuthentication,
    codecs::EncodingConfig,
    config::AcknowledgementsConfig,
    sinks::util::TowerRequestConfig,
    template::{Template, TemplateParseError},
    tls::TlsConfig,
};

#[derive(Debug, Snafu)]
pub(super) enum BuildError {
    #[snafu(display("`message_group_id` should be defined for FIFO queue."))]
    MessageGroupIdMissing,
    #[snafu(display("`message_group_id` is not allowed with non-FIFO queue."))]
    MessageGroupIdNotAllowed,
    #[snafu(display("invalid topic template: {}", source))]
    TopicTemplate { source: TemplateParseError },
}

/// Base Configuration `aws_s_s` for sns and sqs sink.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct BaseSSSinkConfig {
    #[configurable(derived)]
    pub(super) encoding: EncodingConfig,

    /// The tag that specifies that a message belongs to a specific message group.
    ///
    /// Can be applied only to FIFO queues.
    #[configurable(metadata(docs::examples = "vector"))]
    #[configurable(metadata(docs::examples = "vector-%Y-%m-%d"))]
    pub(super) message_group_id: Option<String>,

    /// The message deduplication ID value to allow AWS to identify duplicate messages.
    ///
    /// This value is a template which should result in a unique string for each event. See the [AWS
    /// documentation][deduplication_id_docs] for more about how AWS does message deduplication.
    ///
    /// [deduplication_id_docs]: https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/using-messagededuplicationid-property.html
    #[configurable(metadata(docs::examples = "{{ transaction_id }}"))]
    pub(super) message_deduplication_id: Option<String>,

    #[configurable(derived)]
    #[serde(default)]
    pub(super) request: TowerRequestConfig,

    #[configurable(derived)]
    pub(super) tls: Option<TlsConfig>,

    /// The ARN of an [IAM role][iam_role] to assume at startup.
    ///
    /// [iam_role]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles.html
    #[configurable(deprecated)]
    #[configurable(metadata(docs::hidden))]
    pub(super) assume_role: Option<String>,

    #[configurable(derived)]
    #[serde(default)]
    pub(super) auth: AwsAuthentication,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub(super) acknowledgements: AcknowledgementsConfig,
}

pub(super) fn message_group_id(
    message_group_id: Option<String>,
    fifo: bool,
) -> crate::Result<Option<Template>> {
    match (message_group_id.as_ref(), fifo) {
        (Some(value), true) => Ok(Some(
            Template::try_from(value.clone()).context(TopicTemplateSnafu)?,
        )),
        (Some(_), false) => Err(Box::new(BuildError::MessageGroupIdNotAllowed)),
        (None, true) => Err(Box::new(BuildError::MessageGroupIdMissing)),
        (None, false) => Ok(None),
    }
}
pub(super) fn message_deduplication_id(
    message_deduplication_id: Option<String>,
) -> crate::Result<Option<Template>> {
    Ok(message_deduplication_id
        .clone()
        .map(Template::try_from)
        .transpose()?)
}
