The `gcp_cloud_storage` sink now applies the configured `timezone` option (or the global `timezone`) to `strftime` specifiers in `key_prefix`. Named and local timezones use the offset at each event's timestamp, including daylight saving transitions. Previously, date-based partitioning in `key_prefix` was always rendered in UTC and ignored the `timezone` setting, so object paths could land in the wrong dated directory around the UTC day boundary. This change does not alter `filename_time_format` or the `aws_s3` sink's startup-offset behavior.

authors: xfocus3
