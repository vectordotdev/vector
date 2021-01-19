package metadata

components: sinks: aws_s3: components._aws & {
	title: "AWS S3"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["AWS"]
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       true
				max_bytes:    10000000
				max_events:   null
				timeout_secs: 300
			}
			compression: {
				enabled: true
				default: "gzip"
				algorithms: ["none", "gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: {
					enabled: true
					default: null
					enum: ["ndjson", "text"]
				}
			}
			request: {
				enabled:                    true
				concurrency:                50
				rate_limit_duration_secs:   1
				rate_limit_num:             250
				retry_initial_backoff_secs: 1
				retry_max_duration_secs:    10
				timeout_secs:               30
				headers:                    false
			}
			tls: enabled: false
			to: {
				service: services.aws_s3

				interface: {
					socket: {
						api: {
							title: "AWS S3 API"
							url:   urls.aws_s3_endpoints
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "required"
					}
				}
			}
		}
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		acl: {
			category:    "ACL"
			common:      false
			description: "Canned ACL to apply to the created objects. For more information, see [Canned ACL](\(urls.aws_s3_canned_acl))."
			required:    false
			warnings: []
			type: string: {
				default: null
				enum: {
					"private":                   "Owner gets `FULL_CONTROL`. No one else has access rights (default)."
					"public-read":               "Owner gets `FULL_CONTROL`. The AllUsers group gets `READ` access."
					"public-read-write":         "Owner gets `FULL_CONTROL`. The AllUsers group gets `READ` and `WRITE` access. Granting this on a bucket is generally not recommended."
					"aws-exec-read":             "Owner gets `FULL_CONTROL`. Amazon EC2 gets `READ` access to `GET` an Amazon Machine Image (AMI) bundle from Amazon S3."
					"authenticated-read":        "Owner gets `FULL_CONTROL`. The AuthenticatedUsers group gets `READ` access."
					"bucket-owner-read":         "Object owner gets `FULL_CONTROL`. Bucket owner gets `READ. access."
					"bucket-owner-full-control": "Both the object owner and the bucket owner get `FULL_CONTROL` over the object."
					"log-delivery-write":        "The LogDelivery group gets `WRITE` and `READ_ACP` permissions on the bucket. For more information about logs, see [Amazon S3 Server Access Logging](\(urls.aws_s3_server_access_logs))."
				}
			}
		}
		bucket: {
			description: "The S3 bucket name. Do not include a leading `s3://` or a trailing `/`."
			required:    true
			warnings: []
			type: string: {
				examples: ["my-bucket"]
			}
		}
		content_encoding: {
			category:    "Content Type"
			common:      false
			description: "Specifies what content encodings have been applied to the object and thus what decoding mechanisms must be applied to obtain the media-type referenced by the Content-Type header field. By default calculated from `compression` value."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["gzip"]
			}
		}
		content_type: {
			category:    "Content Type"
			common:      false
			description: "A standard MIME type describing the format of the contents."
			required:    false
			warnings: []
			type: string: {
				default: "text/x-log"
			}
		}
		filename_append_uuid: {
			category:    "File Naming"
			common:      false
			description: "Whether or not to append a UUID v4 token to the end of the file. This ensures there are no name collisions high volume use cases."
			required:    false
			warnings: []
			type: bool: default: true
		}
		filename_extension: {
			category:    "File Naming"
			common:      false
			description: "The filename extension to use in the object name."
			required:    false
			warnings: []
			type: string: {
				default: "log"
			}
		}
		filename_time_format: {
			category:    "File Naming"
			common:      false
			description: "The format of the resulting object file name. [`strftime` specifiers](\(urls.strptime_specifiers)) are supported."
			required:    false
			warnings: []
			type: string: {
				default: "%s"
			}
		}
		grant_full_control: {
			category:    "ACL"
			common:      false
			description: "Gives the named [grantee](\(urls.aws_s3_grantee)) READ, READ_ACP, and WRITE_ACP permissions on the created objects."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be", "person@email.com", "http://acs.amazonaws.com/groups/global/AllUsers"]
			}
		}
		grant_read: {
			category:    "ACL"
			common:      false
			description: "Allows the named [grantee](\(urls.aws_s3_grantee)) to read the created objects and their metadata."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be", "person@email.com", "http://acs.amazonaws.com/groups/global/AllUsers"]
			}
		}
		grant_read_acp: {
			category:    "ACL"
			common:      false
			description: "Allows the named [grantee](\(urls.aws_s3_grantee)) to read the created objects' ACL."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be", "person@email.com", "http://acs.amazonaws.com/groups/global/AllUsers"]
			}
		}
		grant_write_acp: {
			category:    "ACL"
			common:      false
			description: "Allows the named [grantee](\(urls.aws_s3_grantee)) to write the created objects' ACL."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be", "person@email.com", "http://acs.amazonaws.com/groups/global/AllUsers"]
			}
		}
		key_prefix: {
			category:    "File Naming"
			common:      true
			description: "A prefix to apply to all object key names. This should be used to partition your objects, and it's important to end this value with a `/` if you want this to be the root S3 \"folder\"."
			required:    false
			warnings: []
			type: string: {
				default: "date=%F/"
				examples: ["date=%F/", "date=%F/hour=%H/", "year=%Y/month=%m/day=%d/", "application_id={{ application_id }}/date=%F/"]
				templateable: true
			}
		}
		server_side_encryption: {
			category:    "Encryption"
			common:      false
			description: "The Server-side Encryption algorithm used when storing these objects."
			required:    false
			warnings: []
			type: string: {
				default: null
				enum: {
					"AES256":  "256-bit Advanced Encryption Standard"
					"aws:kms": "AWS managed key encryption"
				}
			}
		}
		ssekms_key_id: {
			category:    "Encryption"
			common:      false
			description: "If `server_side_encryption` has the value `\"aws.kms\"`, this specifies the ID of the AWS Key Management Service (AWS KMS) symmetrical customer managed customer master key (CMK) that will used for the created objects. If not specified, Amazon S3 uses the AWS managed CMK in AWS to protect the data."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["abcd1234"]
			}
		}
		storage_class: {
			category:    "Storage"
			common:      false
			description: "The storage class for the created objects. See [the S3 Storage Classes](https://docs.aws.amazon.com/AmazonS3/latest/dev/storage-class-intro.html) for more details."
			required:    false
			warnings: []
			type: string: {
				default: null
				enum: {
					STANDARD:            "The default storage class. If you don't specify the storage class when you upload an object, Amazon S3 assigns the STANDARD storage class."
					REDUCED_REDUNDANCY:  "Designed for noncritical, reproducible data that can be stored with less redundancy than the STANDARD storage class. AWS recommends that you not use this storage class. The STANDARD storage class is more cost effective. "
					INTELLIGENT_TIERING: "Stores objects in two access tiers: one tier that is optimized for frequent access and another lower-cost tier that is optimized for infrequently accessed data."
					STANDARD_IA:         "Amazon S3 stores the object data redundantly across multiple geographically separated Availability Zones (similar to the STANDARD storage class)."
					ONEZONE_IA:          "Amazon S3 stores the object data in only one Availability Zone."
					GLACIER:             "Use for archives where portions of the data might need to be retrieved in minutes."
					DEEP_ARCHIVE:        "Use for archiving data that rarely needs to be accessed."
				}
			}
		}
		tags: {
			common:      false
			description: "The tag-set for the object."
			required:    false
			warnings: []
			type: object: {
				examples: [{"Tag1": "Value1"}]
				options: {}
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	how_it_works: {
		cross_account: {
			title: "Cross account object writing"
			body:  """
				If you're using Vector to write objects across AWS accounts then you should
				consider setting the `grant_full_control` option to the bucket owner's
				canonical user ID. AWS provides a
				[full tutorial](\(urls.aws_s3_cross_account_tutorial)) for this use case. If
				don't know the bucket owner's canonical ID you can find it by following
				[this tutorial](\(urls.aws_canonical_user_id)).
				"""
		}

		object_acl: {
			title: "Object Access Control List (ACL)"
			body:  """
				AWS S3 supports [access control lists (ACL)](\(urls.aws_s3_acl)) for buckets and
				objects. In the context of Vector, only object ACLs are relevant (Vector does
				not create or modify buckets). You can set the object level ACL by using one
				of the `acl`, `grant_full_control`, `grant_read`, `grant_read_acp`, or
				`grant_write_acp` options.
				"""
			sub_sections: [
				{
					title: "`acl.*` vs `grant_*` options"
					body:  """
						The `grant_*` options name a specific entity to grant access to. The `acl`
						options is one of a set of [specific canned ACLs](\(urls.aws_s3_canned_acl)) that
						can only name the owner or world.
						"""
				},
			]
		}

		object_naming: {
			title: "Object naming"
			body:  """
				By default, Vector will name your S3 objects in the following format:

				<Tabs
				  block={true}
				  defaultValue="without_compression"
				  values={[
				    { label: 'Without Compression', value: 'without_compression', },
				    { label: 'With Compression', value: 'with_compression', },
				  ]
				}>

				<TabItem value="without_compression">

				```text
				<key_prefix><timestamp>-<uuidv4>.log
				```

				For example:

				```text
				date=2019-06-18/1560886634-fddd7a0e-fad9-4f7e-9bce-00ae5debc563.log
				```

				</TabItem>
				<TabItem value="with_compression">

				```text
				<key_prefix><timestamp>-<uuidv4>.log.gz
				```

				For example:

				```text
				date=2019-06-18/1560886634-fddd7a0e-fad9-4f7e-9bce-00ae5debc563.log.gz
				```

				</TabItem>
				</Tabs>

				Vector appends a [UUIDV4](\(urls.uuidv4)) token to ensure there are no name
				conflicts in the unlikely event 2 Vector instances are writing data at the same
				time.

				You can control the resulting name via the `key_prefix`, `filename_time_format`,
				and `filename_append_uuid` options.
				"""
		}

		object_tags_and_metadata: {
			title: "Object Tags & metadata"
			body:  """
				Vector currently only supports [AWS S3 object tags](\(urls.aws_s3_tags)) and does
				_not_ support [object metadata](\(urls.aws_s3_metadata)). If you require metadata
				support see [issue #1694](\(urls.issue_1694)).

				We believe tags are more flexible since they are separate from the actual S3
				object. You can freely modify tags without modifying the object. Conversely,
				object metadata requires a full rewrite of the object to make changes.
				"""
		}

		server_side_encryption: {
			title: "Server-Side Encryption (SSE)"
			body:  """
				AWS S3 offers [server-side encryption](\(urls.aws_s3_sse)). You can apply defaults
				at the bucket level or set the encryption at the object level. In the context,
				of Vector only the object level is relevant (Vector does not create or modify
				buckets). Although, we recommend setting defaults at the bucket level whne
				possible. You can explicitly set the object level encryption via the
				`server_side_encryption` option.
				"""
		}

		storage_class: {
			title: "Storage class"
			body:  """
				AWS S3 offers [storage classes](\(urls.aws_s3_storage_classes)). You can apply
				defaults, and rules, at the bucket level or set the storage class at the object
				level. In the context of Vector only the object level is relevant (Vector does
				not create or modify buckets). You can set the storage class via the
				`storage_class` option.
				"""
		}
	}

	permissions: iam: [
		{
			platform:  "aws"
			_service:  "s3"
			_docs_tag: "AmazonS3"

			policies: [
				{
					_action: "HeadBucket"
					required_for: ["healthcheck"]
				},
				{
					_action: "ListBucket"
					required_for: ["healthcheck"]
				},
				{
					_action: "PutObject"
				},
			]
		},
	]
}
