---
title: Using AWS Secrets Manager with Vector
short: AWS Secrets Manager
description: Integrate AWS Secrets Manager with Vector to securely manage sensitive configuration values like API keys and database passwords.
weight: 2
tags: ["aws", "secrets", "security", "authentication"]
domain: enriching
---

AWS Secrets Manager is a fully-managed service that helps you protect secrets needed to access your applications, services, and IT resources. This guide goes over how to integrate AWS Secrets Manager with Vector to securely manage sensitive configuration values like API keys, database passwords, and other credentials.

## Prerequisites

Before you begin, ensure you have:

- An AWS account with access to AWS Secrets Manager.
- Appropriate AWS IAM permissions to read secrets.
- Vector v0.38.0 or higher installed.
- Vector compiled with the `secrets-aws-secrets-manager` feature (enabled by default in most distributions).

## Setting up AWS Secrets Manager

### 1. Create a secret in AWS Secrets Manager

First, create a secret in AWS Secrets Manager that contains your sensitive values as key-value pairs:

```json
{
  "database_password": "super_secret_password",
  "api_key": "your-api-key-here",
  "oauth_token": "your-oauth-token"
}
```

You can create this secret using the AWS Console, AWS CLI, or AWS SDK. Take note of the **Secret ARN** or **Secret Name** because you need it to configure Vector later on.

### 2. Configure AWS credentials

Vector needs AWS credentials to access Secrets Manager. You can provide credentials in several ways:

#### Option 1: Using AWS IAM roles (recommended for EC2/ECS/EKS)

If you are running Vector on AWS infrastructure, use IAM roles attached to your compute resources.

#### Option 2: Using AWS credentials file

```ini
# ~/.aws/credentials
[default]
aws_access_key_id = YOUR_ACCESS_KEY_ID
aws_secret_access_key = YOUR_SECRET_ACCESS_KEY
```

#### Option 3: Using environment variables

```bash
export AWS_ACCESS_KEY_ID=YOUR_ACCESS_KEY_ID
export AWS_SECRET_ACCESS_KEY=YOUR_SECRET_ACCESS_KEY
export AWS_DEFAULT_REGION=us-east-1
```

### 3. Required IAM permissions

Ensure the AWS credentials have the following IAM policy attached:

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
        "arn:aws:secretsmanager:region:account:secret:secret-name-*"
      ]
    }
  ]
}
```

## Configuring Vector

### 1. Configure the AWS Secrets Manager backend

Add the AWS Secrets Manager backend to your Vector configuration:

```yaml
secret:
  my_aws_secrets:
    type: aws_secrets_manager
    secret_id: "my-app-secrets"  # The name or ARN of your secret
    region: "us-east-1"          # Optional: AWS region

    # Optional: Explicit AWS authentication (if not using default credential chain)
    auth:
      access_key_id: "YOUR_ACCESS_KEY_ID"
      secret_access_key: "YOUR_SECRET_ACCESS_KEY"
```

### 2. Use secrets in your Vector configuration

Reference the secrets using the `SECRET[backend.key]` syntax:

```yaml
sources:
  my_database:
    type: postgresql_metrics
    endpoints:
      - "postgresql://user:SECRET[my_aws_secrets.database_password]@localhost:5432/database"

sinks:
  my_api_sink:
    type: http
    uri: "https://api.example.com/events"
    encoding:
      codec: json
    headers:
      Authorization: "Bearer SECRET[my_aws_secrets.oauth_token]"
      X-API-Key: "SECRET[my_aws_secrets.api_key]"
```

### 3. Complete example

Here's a complete example configuration:

```yaml
# AWS Secrets Manager backend configuration
secret:
  production_secrets:
    type: aws_secrets_manager
    secret_id: "vector-production-secrets"
    region: "us-west-2"

# Source that reads from a database using a secret password
sources:
  app_logs:
    type: postgresql_metrics
    endpoints:
      - "postgresql://vector:SECRET[production_secrets.db_password]@db.example.com:5432/logs"

# Sink that sends to an external API using secret API key
sinks:
  external_api:
    type: http
    uri: "https://logs.example.com/v1/events"
    inputs:
      - app_logs
    encoding:
      codec: json
    headers:
      Authorization: "Bearer SECRET[production_secrets.api_token]"
```

## Configuration options

The AWS Secrets Manager backend supports the following configuration options:

| Option                   | Type   | Description                                                                                            |
| ------------------------ | ------ | ------------------------------------------------------------------------------------------------------ |
| `secret_id`              | string | **Required.** The name or ARN of the secret in AWS Secrets Manager.                                    |
| `region`                 | string | AWS region where the secret is stored. If not specified, the default AWS region configuration is used. |
| `auth.access_key_id`     | string | AWS access key ID. Optional, if using default credential chain.                                        |
| `auth.secret_access_key` | string | AWS secret access key. Optional, if using default credential chain.                                    |
| `auth.session_token`     | string | AWS session token for temporary credentials.                                                           |
| `auth.assume_role`       | string | ARN of an IAM role to assume.                                                                          |
| `auth.external_id`       | string | External ID when assuming a role.                                                                      |
| `tls`                    | object | TLS configuration options.                                                                             |

## Secret format

The secret stored in AWS Secrets Manager must be a JSON object with string keys and string values. For example:

```json
{
  "key1": "value1",
  "key2": "value2",
  "database_url": "postgresql://user:pass@host:5432/db",
  "api_key": "your-secret-api-key"
}
```

Vector retrieves the entire secret and makes individual key-value pairs available using the `SECRET[backend_name.key_name]` syntax.

## Security considerations

1. **Least privilege**: Grant only the minimum required IAM permissions (`secretsmanager:GetSecretValue`) for the specific secrets Vector needs to access.

2. **Secret rotation**: AWS Secrets Manager supports automatic secret rotation. Vector fetches the latest secret value each time it starts or reloads its configuration.

3. **Network security**: Ensure Vector can reach the AWS Secrets Manager service endpoints. In VPC environments, you may need VPC endpoints or appropriate routing.

4. **Logging**: Be cautious about logging levels that might expose secret values in Vector logs.

5. **Disk buffers**: When using disk buffers, be aware that secrets may be stored unencrypted on disk. Secure the Vector data directory appropriately.

## Troubleshooting

### Common issues

#### `Backend not found in config` error

- Ensure the backend name in `SECRET[backend_name.key]` exactly matches the section name in your config.

#### `Key does not exist` error

- Verify the key name exists in your AWS Secrets Manager secret.
- Check that the secret contains valid JSON.

#### `Secret could not be retrieved` error

- Verify AWS credentials have the correct permissions.
- Check that the secret ID and ARN are correct.
- Ensure the secret exists in the specified region.

#### AWS authentication errors

- Verify AWS credentials are configured correctly.
- Check IAM permissions.
- Ensure the region is correctly specified.

### Debugging

Enable debug logging to see more details about secret retrieval:

```yaml
api:
  enabled: true
  address: "0.0.0.0:8686"

# Add debug logging
log:
  level: debug
```

Check Vector logs for messages related to secret retrieval:

```bash
vector --config vector.yaml 2>&1 | grep -i secret
```

## Best practices

1. **Use descriptive backend names**: Choose meaningful names for your secret backends that clearly indicate their purpose.

2. **Group related secrets**: Store related secrets together in the same AWS Secrets Manager secret to minimize the number of API calls.

3. **Handle secret rotation**: Design your application to handle secret rotation gracefully. Vector fetches secrets at startup and configuration reload.

4. **Monitor access**: Use AWS CloudTrail to monitor access to your secrets and set up alerts for unexpected access patterns.

5. **Use separate secrets for different environments**: Maintain separate secrets for development, staging, and production environments.

## Related resources

- [AWS Secrets Manager Documentation](https://docs.aws.amazon.com/secretsmanager/)
- [Vector Secrets Management Overview](/docs/reference/configuration/global-options/#secret)
- [AWS Authentication in Vector](/docs/reference/configuration/components/aws/)
