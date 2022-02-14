use rusoto_core::RusotoError;
use rusoto_logs::{CloudWatchLogs, CloudWatchLogsClient, DescribeLogGroupsRequest};
use snafu::Snafu;

use crate::sinks::aws_cloudwatch_logs::config::CloudwatchLogsSinkConfig;

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("DescribeLogGroups failed: {}", source))]
    DescribeLogGroupsFailed {
        source: RusotoError<rusoto_logs::DescribeLogGroupsError>,
    },
    #[snafu(display("No log group found"))]
    NoLogGroup,
    #[snafu(display("Unable to extract group name"))]
    GroupNameError,
    #[snafu(display("Group name mismatch: expected {}, found {}", expected, name))]
    GroupNameMismatch { expected: String, name: String },
}

pub async fn healthcheck(
    config: CloudwatchLogsSinkConfig,
    client: CloudWatchLogsClient,
) -> crate::Result<()> {
    let group_name = config.group_name.get_ref().to_owned();
    let expected_group_name = group_name.clone();

    let request = DescribeLogGroupsRequest {
        limit: Some(1),
        log_group_name_prefix: Some(group_name),
        ..Default::default()
    };

    // This will attempt to find the group name passed in and verify that
    // it matches the one that AWS sends back.
    match client.describe_log_groups(request).await {
        Ok(resp) => match resp.log_groups.and_then(|g| g.into_iter().next()) {
            Some(group) => {
                if let Some(name) = group.log_group_name {
                    if name == expected_group_name {
                        Ok(())
                    } else {
                        Err(HealthcheckError::GroupNameMismatch {
                            expected: expected_group_name,
                            name,
                        }
                        .into())
                    }
                } else {
                    Err(HealthcheckError::GroupNameError.into())
                }
            }
            None => {
                if config.group_name.is_dynamic() {
                    info!("Skipping healthcheck log group check: `group_name` is dynamic.");
                    Ok(())
                } else if config.create_missing_group.unwrap_or(true) {
                    info!("Skipping healthcheck log group check: `group_name` will be created if missing.");
                    Ok(())
                } else {
                    Err(HealthcheckError::NoLogGroup.into())
                }
            }
        },
        Err(source) => Err(HealthcheckError::DescribeLogGroupsFailed { source }.into()),
    }
}
