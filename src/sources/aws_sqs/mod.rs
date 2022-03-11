mod config;
mod events;
mod integration_tests;
mod source;

use config::AwsSqsConfig;

use crate::config::SourceDescription;

pub use config::SqsClientBuilder;

inventory::submit! {
    SourceDescription::new::<AwsSqsConfig>("aws_sqs")
}
