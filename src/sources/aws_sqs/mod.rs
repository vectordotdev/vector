mod config;
mod events;
mod integration_tests;
mod source;

use crate::config::SourceDescription;
use config::AwsSqsConfig;

inventory::submit! {
    SourceDescription::new::<AwsSqsConfig>("aws_sqs")
}
