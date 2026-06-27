The `aws_cloudwatch_logs` sink now parses string- and integer-encoded `.timestamp` values instead of silently replacing them with the current time. Integer values are interpreted as Unix seconds. If the value cannot be parsed, the sink falls back to the current time and emits a warning.

authors: shivansh-mathur
