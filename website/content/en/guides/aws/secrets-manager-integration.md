---
title: AWS Secrets Manager Integration Example
weight: 8
---

This document goes over how to use Vector with AWS Secrets Manager to securely manage credentials for various AWS services and external APIs.

## Scenario

In this example, Vector is set up to:

1. Read logs from Amazon S3.
2. Send metrics to Amazon CloudWatch.
3. Forward logs to an external API.
4. Store database credentials and API keys securely in AWS Secrets Manager.

## Prerequisites

- An AWS account with appropriate permissions
- AWS CLI configured
- Vector v0.38.0 or higher installed with AWS Secrets Manager support

## Step 1: Create secrets in AWS Secrets Manager

First, create a secret containing all the sensitive values:

```bash
aws secretsmanager create-secret \
  --name "vector-production-credentials" \
  --description "Credentials for Vector production deployment" \
  --secret-string '{
    "s3_access_key": "AKIA...",
    "s3_secret_key": "your-s3-secret-key",
    "external_api_token": "your-external-api-token",
    "database_password": "your-database-password",
    "webhook_secret": "your-webhook-secret"
  }' \
  --region us-west-2
```

## Step 2: Configure IAM permissions

Create an IAM policy that allows Vector to read the secret:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "secretsmanager:GetSecretValue"
      ],
      "Resource": [
        "arn:aws:secretsmanager:us-west-2:123456789012:secret:vector-production-credentials-*"
      ]
    },
    {
      "Effect": "Allow",
      "Action": [
        "s3:GetObject",
        "s3:ListBucket"
      ],
      "Resource": [
        "arn:aws:s3:::your-logs-bucket",
        "arn:aws:s3:::your-logs-bucket/*"
      ]
    },
    {
      "Effect": "Allow",
      "Action": [
        "cloudwatch:PutMetricData"
      ],
      "Resource": "*"
    }
  ]
}
```

Attach this policy to the IAM role or user that Vector will use.

## Step 3: Vector configuration

Create your Vector configuration file:

```yaml
# vector.yaml

# Configure AWS Secrets Manager backend
secret:
  aws_creds:
    type: aws_secrets_manager
    secret_id: "vector-production-credentials"
    region: "us-west-2"

# Source: Read logs from S3
sources:
  s3_logs:
    type: aws_s3
    region: "us-west-2"
    bucket: "your-logs-bucket"
    key_prefix: "application-logs/"
    # Use secrets for S3 authentication
    auth:
      access_key_id: "SECRET[aws_creds.s3_access_key]"
      secret_access_key: "SECRET[aws_creds.s3_secret_key]"

  # Source: Internal metrics
  internal_metrics:
    type: internal_metrics

# Transform: Parse and enrich logs
transforms:
  parse_logs:
    type: remap
    inputs:
      - s3_logs
    source: |
      . = parse_json!(.message)
      .timestamp = now()
      .environment = "production"

  # Transform: Generate custom metrics
  generate_metrics:
    type: log_to_metric
    inputs:
      - parse_logs
    metrics:
      - type: counter
        field: level
        name: log_events_total
        namespace: application
        tags:
          level: "{{ level }}"
          service: "{{ service }}"

# Sink: Send metrics to CloudWatch
sinks:
  cloudwatch_metrics:
    type: aws_cloudwatch_metrics
    inputs:
      - internal_metrics
      - generate_metrics
    namespace: "Vector/Application"
    region: "us-west-2"

  # Sink: Forward logs to external API
  external_api:
    type: http
    inputs:
      - parse_logs
    uri: "https://logs.example.com/v1/ingest"
    encoding:
      codec: json
    compression: gzip
    # Use secret for API authentication
    headers:
      Authorization: "Bearer SECRET[aws_creds.external_api_token]"
      X-API-Version: "v1"
    # Batch logs for efficiency
    batch:
      max_bytes: 1048576  # 1MB
      timeout_secs: 30

  # Sink: Store processed logs in S3 for archival
  s3_archive:
    type: aws_s3
    inputs:
      - parse_logs
    bucket: "your-archive-bucket"
    key_prefix: "processed-logs/%Y/%m/%d/"
    region: "us-west-2"
    compression: gzip
    encoding:
      codec: ndjson
    # Use the same S3 credentials from secrets
    auth:
      access_key_id: "SECRET[aws_creds.s3_access_key]"
      secret_access_key: "SECRET[aws_creds.s3_secret_key]"

  # Optional: Database source using secret password
  postgres_metrics:
    type: postgresql_metrics
    endpoints:
      - "postgresql://vector:SECRET[aws_creds.database_password]@postgres.internal:5432/metrics"
    scrape_interval_secs: 60

  # Optional: Webhook source with secret validation
  webhook:
    type: http_server
    address: "0.0.0.0:8080"
    encoding: json

# Transform: Validate webhook signature
  validate_webhook:
    type: remap
    inputs:
      - webhook
    source: |
      expected_signature = hmac_sha256(string!(.message), "SECRET[aws_creds.webhook_secret]")
      if .headers."x-signature" != expected_signature {
        abort
      }
```

## Step 4: Deploy Vector

Deploy Vector with the configuration:

```bash
# Run Vector
vector --config vector.yaml

# Or as a service
sudo systemctl start vector
```

## Step 5: Monitor and validate

Check that Vector is successfully reading secrets:

```bash
# Check Vector logs
journalctl -u vector -f

# Verify metrics are being sent to CloudWatch
aws cloudwatch list-metrics --namespace "Vector/Application"

# Check S3 for archived logs
aws s3 ls s3://your-archive-bucket/processed-logs/
```

## Security considerations

1. **IAM permissions**: Use the principle of least privilege
2. **Secret rotation**: Set up automatic rotation in AWS Secrets Manager
3. **Network security**: Use VPC endpoints for AWS services when possible
4. **Monitoring**: Enable CloudTrail logging for secret access
5. **Backup**: Consider cross-region secret replication for disaster recovery

## Troubleshooting

### Common issues and solutions

1. **Permission denied errors**
   - Verify IAM permissions include `secretsmanager:GetSecretValue`
   - Check that the secret ARN is correct
   - Ensure Vector is running with the correct IAM role/credentials

2. **Secret not found**
   - Verify the secret name/ID is correct
   - Ensure the secret exists in the specified region
   - Check for typos in the `secret_id` configuration

3. **Invalid JSON in secret**
   - Validate that your secret value is valid JSON
   - Ensure all values are strings
   - Check for special characters that need escaping

4. **Network connectivity issues**
   - Verify Vector can reach AWS Secrets Manager endpoints
   - Check security groups and NACLs
   - Consider using VPC endpoints for private connectivity

## Advanced configuration

### Using assume role

If Vector needs to assume a different role to access secrets:

```yaml
secret:
  aws_creds:
    type: aws_secrets_manager
    secret_id: "vector-production-credentials"
    region: "us-west-2"
    auth:
      assume_role: "arn:aws:iam::123456789012:role/VectorSecretsRole"
      external_id: "unique-external-id"
```

### Cross-region secrets

For multi-region deployments:

```yaml
# Primary region secrets
secret:
  us_west_2_creds:
    type: aws_secrets_manager
    secret_id: "vector-prod-us-west-2"
    region: "us-west-2"

  # Backup region secrets
  us_east_1_creds:
    type: aws_secrets_manager
    secret_id: "vector-prod-us-east-1"
    region: "us-east-1"
```

This example demonstrates a production-ready setup using AWS Secrets Manager with Vector, providing secure credential management across multiple AWS services and external integrations.
