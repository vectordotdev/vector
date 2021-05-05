Vector checks for AWS credentials in the following order:

1. Options [`access_key_id`](#access_key_id) and [`secret_access_key`](#secret-access-key).
1. Environment variables [`AWS_ACCESS_KEY_ID`](#AWS_ACCESS_KEY_ID) and [`AWS_SECRET_ACCESS_KEY`](#AWS_SECRET_ACCESS_KEY).
1. The [`credential_process`][credential_process] command in the AWS config file (usually located at `~/.aws/config`).
1. The [AWS credentials file][aws_creds] (usually located at `~/.aws/credentials`).
1. The [IAM instance profile][iam_profile] (only works if running on an EC2 instance with an instance profile/role).

If credentials aren't found, the healtcheck fails and an error is logged.

#### Obtaining an access key

In general, we recommend using instance profiles/roles whenever possible. In cases where this isn't possible, you can generate an AWS access key for any user within your AWS account. AWS provides a [detailed guide][access_keys] on this. Such created AWS access keys can be used via the [`access_key_id`](#access_key_id) and [`secret_access_key`](#secret_access_key) options.

#### Assuming roles

Vector can assume an AWS IAM role via the [`assume_role`](#assume_role) option. This optional setting is helpful for a variety of use cases, such as cross-account access.

[access_keys]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_access-keys.html
[aws_creds]: https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-files.html
[credential_process]: https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-sourcing-external.html
[iam_profile]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles_use_switch-role-ec2_instance-profiles.html
