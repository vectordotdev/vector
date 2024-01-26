package metadata

components: _aws: {
	configuration: {
		_aws_include: bool | *true

		if _aws_include {
			auth: {
				common:      false
				description: "Options for the authentication strategy."
				required:    false
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
							}
						}
						region: {
							category:    "Auth"
							common:      false
							description: "The [AWS region](\(urls.aws_regions)) to send STS requests to. If not set, this will default to the configured region for the service itself."
							required:    false
							type: string: {
								default: null
								examples: ["us-west-2"]
							}
						}
						load_timeout_secs: {
							category:    "Auth"
							common:      false
							description: "The timeout for loading credentials. Relevant when the default credentials chain is used or `assume_role`."
							required:    false
							type: uint: {
								unit:    "seconds"
								default: 5
								examples: [30]
							}
						}
						imds: {
							description: "Configuration for authenticating with AWS through IMDS."
							required:    false
							type: object: {
								options: {
									connect_timeout_seconds: {
										description: "Connect timeout for IMDS."
										required:    false
										type: uint: {
											default: 1
											unit:    "seconds"
										}
									}
									max_attempts: {
										description: "Number of IMDS retries for fetching tokens and metadata."
										required:    false
										type: uint: default: 4
									}
									read_timeout_seconds: {
										description: "Read timeout for IMDS."
										required:    false
										type: uint: {
											default: 1
											unit:    "seconds"
										}
									}
								}
							}
						}
						profile: {
							category:    "Auth"
							common:      false
							description: "The AWS profile name. Used to select AWS credentials from a provided credentials file."
							required:    false
							type: string: {
								default: "default"
								examples: ["develop"]
							}
						}
					}
				}
			}

			endpoint: {
				common:      false
				description: "Custom endpoint for use with AWS-compatible services."
				required:    false
				type: string: {
					default: null
					examples: ["http://127.0.0.0:5000/path/to/service"]
				}
			}
			region: {
				description: "The [AWS region](\(urls.aws_regions)) of the target service."
				required:    true
				type: string: {
					examples: ["us-east-1"]
				}
			}
		}
	}

	env_vars: {
		AWS_ACCESS_KEY_ID: {
			description: "The AWS access key id. Used for AWS authentication when communicating with AWS services."
			type: string: {
				default: null
				examples: ["AKIAIOSFODNN7EXAMPLE"]
			}
		}

		AWS_CONFIG_FILE: {
			description: "Specifies the location of the file that the AWS CLI uses to store configuration profiles."
			type: string: {
				default: "~/.aws/config"
			}
		}

		AWS_DEFAULT_REGION: {
			description:   "The default [AWS region](\(urls.aws_regions))."
			relevant_when: "endpoint = null"
			type: string: {
				default: null
				examples: ["/path/to/credentials.json"]
			}
		}

		AWS_PROFILE: {
			description: "Specifies the name of the CLI profile with the credentials and options to use. This can be the name of a profile stored in a credentials or config file."
			type: string: {
				default: "default"
				examples: ["my-custom-profile"]
			}
		}

		AWS_ROLE_SESSION_NAME: {
			description: "Specifies a name to associate with the role session. This value appears in CloudTrail logs for commands performed by the user of this profile."
			type: string: {
				default: null
				examples: ["vector-session"]
			}
		}

		AWS_SECRET_ACCESS_KEY: {
			description: "The AWS secret access key. Used for AWS authentication when communicating with AWS services."
			type: string: {
				default: null
				examples: ["wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"]
			}
		}

		AWS_SESSION_TOKEN: {
			description: "The AWS session token. Used for AWS authentication when communicating with AWS services."
			type: string: {
				default: null
				examples: ["AQoEXAMPLEH4aoAH0gNCAPy...truncated...zrkuWJOgQs8IZZaIv2BXIa2R4Olgk"]
			}
		}

		AWS_SHARED_CREDENTIALS_FILE: {
			description: "Specifies the location of the file that the AWS CLI uses to store access keys."
			type: string: {
				default: "~/.aws/credentials"
			}
		}
	}

	how_it_works: {
		aws_authentication: {
			title: "AWS authentication"
			body:  """
				Vector checks for AWS credentials in the following order:

				1. The [`auth.access_key_id`](#auth.access_key_id) and
				   [`auth.secret_access_key`](#auth.secret_access_key) options.
				2. The [`AWS_ACCESS_KEY_ID`](#auth.access_key_id) and
				   [`AWS_SECRET_ACCESS_KEY`](#auth.secret_access_key) environment variables.
				3. In [Web Identity
				   Token](https://docs.aws.amazon.com/STS/latest/APIReference/API_AssumeRoleWithWebIdentity.html)
				   credentials from the environment or container (including EKS). These credentials
				   will automatically refresh when expired.
				4. [ECS credentials (IAM roles for
				   tasks)](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task-iam-roles.html).
				   These credentials will automatically refresh when expired.
				5. As entries in the credentials file in the `.aws` directory in your home directory
				   (`~/.aws/credentials` on Linux, OS X, and Unix; `%userprofile%\\.aws\\credentials`
				   on Microsoft Windows).
				6. Using a named profile specified in the credentials file via the AWS_PROFILE environment variable.
				7. The [IAM instance profile](\(urls.iam_instance_profile)) (only works if running on an EC2 instance
				   with an instance profile/role). Requires IMDSv2 to be enabled. For EKS, you may need to increase the
				   metadata token response hop limit to 2. These credentials will automatically refresh when expired.

				If no credentials are found, Vector's health check fails and an error is [logged](\(urls.vector_monitoring)).
				"""
			sub_sections: [
				{
					title: "Obtaining an access key"
					body:  """
						In general, we recommend using instance profiles/roles whenever possible. In
						cases where this is not possible you can generate an AWS access key for any user
						within your AWS account. AWS provides a [detailed guide](\(urls.aws_access_keys)) on
						how to do this. Such created AWS access keys can be used via [`auth.access_key_id`](#auth.access_key_id)
						and [`auth.secret_access_key`](#auth.secret_access_key) options.
						"""
				},
				{
					title: "Assuming roles"
					body: """
						Vector can assume an AWS IAM role via the [`auth.assume_role`](#auth.assume_role) option. This is an
						optional setting that is helpful for a variety of use cases, such as cross
						account access.
						"""
				},
			]
		}
	}
}
