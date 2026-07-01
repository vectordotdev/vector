Fix AWS Cloudwatch Logs sink healthcheck: if a log group name is templated, exit the healthcheck with Ok instead of attempting to verify that it exists
