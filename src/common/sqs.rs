use crate::aws::ClientBuilder;
use aws_sdk_sqs::{Endpoint, Region};
use aws_smithy_async::rt::sleep::AsyncSleep;
use aws_smithy_client::erase::DynConnector;
use aws_types::credentials::SharedCredentialsProvider;
use std::sync::Arc;

pub(crate) struct SqsClientBuilder;

impl ClientBuilder for SqsClientBuilder {
    type ConfigBuilder = aws_sdk_sqs::config::Builder;
    type Client = aws_sdk_sqs::Client;

    fn create_config_builder(
        credentials_provider: SharedCredentialsProvider,
    ) -> Self::ConfigBuilder {
        aws_sdk_sqs::config::Builder::new().credentials_provider(credentials_provider)
    }

    fn with_endpoint_resolver(
        builder: Self::ConfigBuilder,
        endpoint: Endpoint,
    ) -> Self::ConfigBuilder {
        builder.endpoint_resolver(endpoint)
    }

    fn with_region(builder: Self::ConfigBuilder, region: Region) -> Self::ConfigBuilder {
        builder.region(region)
    }

    fn with_sleep_impl(
        builder: Self::ConfigBuilder,
        sleep_impl: Arc<dyn AsyncSleep>,
    ) -> Self::ConfigBuilder {
        builder.sleep_impl(sleep_impl)
    }

    fn client_from_conf_conn(
        builder: Self::ConfigBuilder,
        connector: DynConnector,
    ) -> Self::Client {
        Self::Client::from_conf_conn(builder.build(), connector)
    }
}
