mod config;
mod source;

use crate::config::SourceDescription;
use config::AwsSqsConfig;

inventory::submit! {
    SourceDescription::new::<AwsSqsConfig>("aws_sqs")
}
