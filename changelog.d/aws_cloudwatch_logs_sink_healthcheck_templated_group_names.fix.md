The `aws_cloudwatch_logs` sink skips the log-group healthcheck for templated
`group_name` values, avoiding invalid CloudWatch requests during startup and
`vector validate`.

authors: iandelahorne
