mod config;
mod integration_tests;
mod source;

pub use config::AwsSqsConfig;

use crate::config::SourceDescription;

inventory::submit! {
    SourceDescription::new::<AwsSqsConfig>("aws_sqs")
}
