package metadata

import (
	"strings"
)

components: sinks: [Name=string]: {
	if strings.HasPrefix(Name, "aws_") {
		configuration: {
			assume_role: {
				common:      false
				description: "The ARN of an [IAM role](\(urls.aws_iam_role)) to assume at startup."
				required:    false
				type: string: {
					default: null
					examples: ["arn:aws:iam::123456789098:role/my_role"]
				}
			}

			endpoint: {
				common:      false
				description: "Custom endpoint for use with AWS-compatible services. Providing a value for this option will make `region` moot."
				required:    false
				type: string: {
					default: null
					examples: ["127.0.0.0:5000/path/to/service"]
				}
			}

			region: {
				description: "The [AWS region](\(urls.aws_regions)) of the target service. If `endpoint` is provided it will override this value since the endpoint includes the region."
				required:    true
				type: string: {
					examples: ["us-east-1"]
				}
			}
		}
	}
}
