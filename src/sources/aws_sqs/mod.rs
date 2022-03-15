mod config;
mod events;
mod integration_tests;
mod source;

use config::AwsSqsConfig;

use crate::config::SourceDescription;

inventory::submit! {
    SourceDescription::new::<AwsSqsConfig>("aws_sqs")
}
