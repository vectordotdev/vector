The `gcp_cloud_storage` sink now applies the configured `timezone` option (or the global `timezone`) to `strftime` specifiers in `key_prefix`, matching the behavior of the `aws_s3` sink. Previously, date-based partitioning in `key_prefix` (for example `key_prefix = "%Y%m%d/"`) was always rendered in UTC and ignored the `timezone` setting, so object paths could land in the wrong dated directory around the UTC day boundary.

authors: xfocus3
