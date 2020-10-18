package metadata

import (
	"strings"
)

components: sinks: [Name=string]: {
	if strings.HasPrefix(Name, "aws_") {
		configuration: {
			assume_role: {
				category:    "Auth"
				common:      false
				description: "The ARN of an [IAM role](\(urls.aws_iam_role)) to assume at startup."
				required:    false
				type: string: {
					default: null
					examples: ["arn:aws:iam::123456789098:role/my_role"]
				}
			}

			endpoint: {
				common:        false
				description:   "Custom endpoint for use with AWS-compatible services. Providing a value for this option will make `region` moot."
				relevant_when: "region = null"
				required:      false
				type: string: {
					default: null
					examples: ["127.0.0.0:5000/path/to/service"]
				}
			}

			region: {
				description:   "The [AWS region](\(urls.aws_regions)) of the target service. If `endpoint` is provided it will override this value since the endpoint includes the region."
				required:      true
				relevant_when: "endpoint = null"
				type: string: {
					examples: ["us-east-1"]
				}
			}
		}

		env_vars: {
			AWS_ACCESS_KEY_ID: {
				description:   "Used for AWS authentication when communicating with AWS services."
				relevant_when: "endpoint = null"
				type: string: {
					examples: ["AKIAIOSFODNN7EXAMPLE"]
				}
			}

			AWS_CREDENTIAL_EXPIRATION: {
				description:   "Expiration time in RFC 3339 format (e.g. 1996-12-19T16:39:57-08:00). If unset, credentials won't expire."
				relevant_when: "endpoint = null"
				type: string: {
					examples: ["/path/to/credentials.json"]
				}
			}

			AWS_SECRET_ACCESS_KEY: {
				description:   "Used for AWS authentication when communicating with AWS services."
				relevant_when: "endpoint = null"
				type: string: {
					examples: ["/path/to/credentials.json"]
				}
			}

			AWS_SESSION_TOKEN: {
				description:   "Used for AWS authentication when communicating with AWS services."
				relevant_when: "endpoint = null"
				type: string: {
					examples: ["/path/to/credentials.json"]
				}
			}
		}

		how_it_works: {
			aws_authentication: {
				title: "AWS Authentication"
				body:  """
						Vector checks for AWS credentials in the following order:

						1. Environment variables `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`.
						2. The [`credential_process` command](\(urls.aws_credential_process)) in the AWS config file. (usually located at `~/.aws/config`)
						3. The [AWS credentials file](\(urls.aws_credentials_file)). (usually located at `~/.aws/credentials`)
						4. The [IAM instance profile](\(urls.iam_instance_profile)). (will only work if running on an EC2 instance with an instance profile/role)

						If credentials are not found the [healtcheck](#healthchecks) will fail and an
						error will be [logged][docs.monitoring#logs].
						"""
				sub_sections: [
					{
						title: "Obtaining an access key"
						body:  """
								In general, we recommend using instance profiles/roles whenever possible. In
								cases where this is not possible you can generate an AWS access key for any user
								within your AWS account. AWS provides a [detailed guide](\(urls.aws_access_keys)) on
								how to do this.
								"""
					},
					{
						title: "Assuming roles"
						body: """
								Vector can assume an AWS IAM role via the [`assume_role`](#assume_role) option. This is an
								optional setting that is helpful for a variety of use cases, such as cross
								account access.
								"""
					},
				]
			}
		}
	}
}
