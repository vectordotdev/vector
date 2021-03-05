package metadata

components: _aws: {
	configuration: {
		auth: {
			common:      false
			description: "Options for the authentication strategy."
			required:    false
			warnings: []
			type: object: {
				examples: []
				options: {
					access_key_id: {
						category:    "Auth"
						common:      false
						description: "The AWS access key id. Used for AWS authentication when communicating with AWS services."
						required:    false
						type: string: {
							default: null
							examples: ["AKIAIOSFODNN7EXAMPLE"]
							syntax: "literal"
						}
					}
					secret_access_key: {
						category:    "Auth"
						common:      false
						description: "The AWS secret access key. Used for AWS authentication when communicating with AWS services."
						required:    false
						type: string: {
							default: null
							examples: ["wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"]
							syntax: "literal"
						}
					}
					assume_role: {
						category:    "Auth"
						common:      false
						description: "The ARN of an [IAM role](\(urls.aws_iam_role)) to assume at startup."
						required:    false
						type: string: {
							default: null
							examples: ["arn:aws:iam::123456789098:role/my_role"]
							syntax: "literal"
						}
					}
				}
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
				syntax: "literal"
			}
		}

		region: {
			description:   "The [AWS region](\(urls.aws_regions)) of the target service. If `endpoint` is provided it will override this value since the endpoint includes the region."
			required:      true
			relevant_when: "endpoint = null"
			type: string: {
				examples: ["us-east-1"]
				syntax: "literal"
			}
		}
	}

	env_vars: {
		AWS_ACCESS_KEY_ID: {
			description: "The AWS access key id. Used for AWS authentication when communicating with AWS services."
			type: string: {
				default: null
				examples: ["AKIAIOSFODNN7EXAMPLE"]
				syntax: "literal"
			}
		}

		AWS_CONFIG_FILE: {
			description: "Specifies the location of the file that the AWS CLI uses to store configuration profiles."
			type: string: {
				default: "~/.aws/config"
				syntax:  "literal"
			}
		}

		AWS_CREDENTIAL_EXPIRATION: {
			description: "Expiration time in RFC 3339 format. If unset, credentials won't expire."
			type: string: {
				default: null
				examples: ["1996-12-19T16:39:57-08:00"]
				syntax: "literal"
			}
		}

		AWS_DEFAULT_REGION: {
			description:   "The default [AWS region](\(urls.aws_regions))."
			relevant_when: "endpoint = null"
			type: string: {
				default: null
				examples: ["/path/to/credentials.json"]
				syntax: "literal"
			}
		}

		AWS_PROFILE: {
			description: "Specifies the name of the CLI profile with the credentials and options to use. This can be the name of a profile stored in a credentials or config file."
			type: string: {
				default: "default"
				examples: ["my-custom-profile"]
				syntax: "literal"
			}
		}

		AWS_ROLE_SESSION_NAME: {
			description: "Specifies a name to associate with the role session. This value appears in CloudTrail logs for commands performed by the user of this profile."
			type: string: {
				default: null
				examples: ["vector-session"]
				syntax: "literal"
			}
		}

		AWS_SECRET_ACCESS_KEY: {
			description: "The AWS secret access key. Used for AWS authentication when communicating with AWS services."
			type: string: {
				default: null
				examples: ["wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"]
				syntax: "literal"
			}
		}

		AWS_SHARED_CREDENTIALS_FILE: {
			description: "Specifies the location of the file that the AWS CLI uses to store access keys."
			type: string: {
				default: "~/.aws/credentials"
				syntax:  "literal"
			}
		}

		AWS_SESSION_TOKEN: {
			description: "The AWS session token. Used for AWS authentication when communicating with AWS services."
			type: string: {
				default: null
				examples: ["/path/to/credentials.json"]
				syntax: "literal"
			}
		}
	}

	how_it_works: {
		aws_authentication: {
			title: "AWS Authentication"
			body:  """
				Vector checks for AWS credentials in the following order:

				1. Options [`access_key_id`](#access_key_id) and [`secret_access_key`](#secret_access_key).
				2. Environment variables `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`.
				3. The [`credential_process` command](\(urls.aws_credential_process)) in the AWS config file. (usually located at `~/.aws/config`)
				4. The [AWS credentials file](\(urls.aws_credentials_file)). (usually located at `~/.aws/credentials`)
				5. The [IAM instance profile](\(urls.iam_instance_profile)). (will only work if running on an EC2 instance with an instance profile/role)

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
						how to do this. Such created AWS access keys can be used via [`access_key_id`](#access_key_id)
						and [`secret_access_key`](#secret_access_key) options.
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
