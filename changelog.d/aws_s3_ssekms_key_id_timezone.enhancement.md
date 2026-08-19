The `ssekms_key_id` option in the `aws_s3` sink now respects the configured timezone when the
value is a template containing time components, matching the existing behavior of `key_prefix`.

authors: thomasqueirozb
