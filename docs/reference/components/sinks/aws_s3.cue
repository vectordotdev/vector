package metadata

components: sinks: aws_s3: {
  title: "#{component.title}"
  short_description: "Batches log events to [Amazon Web Service's S3 service][urls.aws_s3] via the [`PutObject` API endpoint](https://docs.aws.amazon.com/AmazonS3/latest/API/RESTObjectPUT.html)."
  long_description: "[Amazon Simple Storage Service (Amazon S3)][urls.aws_s3] is a scalable, high-speed, web-based cloud storage service designed for online backup and archiving of data and applications on Amazon Web Services. It is very commonly used to store log data."

  _features: {
    batch: {
      enabled: true
      common: true,
      max_bytes: 10000000,
      max_events: null,
      timeout_secs: 300
    }
    buffer: enabled: true
    checkpoint: enabled: false
    compression: {
      enabled: true
      default: "gzip"
      gzip: true
    }
    encoding: {
      enabled: true
      default: null
      ndjson: null
      text: null
    }
    healthcheck: enabled: true
    multiline: enabled: false
    request: {
      enabled: true
      common: false,
      in_flight_limit: 50,
      rate_limit_duration_secs: 1,
      rate_limit_num: 250,
      retry_initial_backoff_secs: 1,
      retry_max_duration_secs: 10,
      timeout_secs: 30
    }
    tls: enabled: false
  }

  classes: {
    commonly_used: true
    function: "transmit"
    service_providers: ["AWS"]
  }

  statuses: {
    delivery: "at_least_once"
    development: "stable"
  }

  support: {
      input_types: ["log"]

    platforms: {
      "aarch64-unknown-linux-gnu": true
      "aarch64-unknown-linux-musl": true
      "x86_64-apple-darwin": true
      "x86_64-pc-windows-msv": true
      "x86_64-unknown-linux-gnu": true
      "x86_64-unknown-linux-musl": true
    }

    requirements: []
    warnings: []
  }

  configuration: {
    acl: {
      common: false
      description: "Canned ACL to apply to the created objects. For more information, see [Canned ACL][urls.aws_s3_canned_acl]."
      required: false
      warnings: []
      type: string: {
        default: null
        enum: {
          private: "Owner gets FULL_CONTROL. No one else has access rights (default)."
          public-read: "Owner gets FULL_CONTROL. The AllUsers group gets READ access."
          public-read-write: "Owner gets FULL_CONTROL. The AllUsers group gets READ and WRITE access. Granting this on a bucket is generally not recommended."
          aws-exec-read: "Owner gets FULL_CONTROL. Amazon EC2 gets READ access to GET an Amazon Machine Image (AMI) bundle from Amazon S3."
          authenticated-read: "Owner gets FULL_CONTROL. The AuthenticatedUsers group gets READ access."
          log-delivery-write: "The LogDelivery group gets WRITE and READ_ACP permissions on the bucket. For more information about logs, see [Amazon S3 Server Access Logging](https://docs.aws.amazon.com/AmazonS3/latest/dev/ServerLogs.html)."
        }
      }
    }
    bucket: {
      common: true
      description: "The S3 bucket name. Do not include a leading `s3://` or a trailing `/`."
      required: true
      warnings: []
      type: string: {
        examples: ["my-bucket"]
      }
    }
    content_encoding: {
      common: false
      description: "Specifies what content encodings have been applied to the object and thus what decoding mechanisms must be applied to obtain the media-type referenced by the Content-Type header field. By default calculated from `compression` value."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["gzip"]
      }
    }
    content_type: {
      common: false
      description: "A standard MIME type describing the format of the contents."
      required: false
      warnings: []
      type: string: {
        default: "text/x-log"
      }
    }
    filename_append_uuid: {
      common: false
      description: "Whether or not to append a UUID v4 token to the end of the file. This ensures there are no name collisions high volume use cases."
      required: false
      warnings: []
      type: bool: default: true
    }
    filename_extension: {
      common: false
      description: "The filename extension to use in the object name."
      required: false
      warnings: []
      type: string: {
        default: "log"
      }
    }
    filename_time_format: {
      common: false
      description: "The format of the resulting object file name. [`strftime` specifiers][urls.strptime_specifiers] are supported."
      required: false
      warnings: []
      type: string: {
        default: "%s"
      }
    }
    grant_full_control: {
      common: false
      description: "Gives the named [grantee][urls.aws_s3_grantee] READ, READ_ACP, and WRITE_ACP permissions on the created objects."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be","person@email.com","http://acs.amazonaws.com/groups/global/AllUsers"]
      }
    }
    grant_read: {
      common: false
      description: "Allows the named [grantee][urls.aws_s3_grantee] to read the created objects and their metadata."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be","person@email.com","http://acs.amazonaws.com/groups/global/AllUsers"]
      }
    }
    grant_read_acp: {
      common: false
      description: "Allows the named [grantee][urls.aws_s3_grantee] to read the created objects' ACL."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be","person@email.com","http://acs.amazonaws.com/groups/global/AllUsers"]
      }
    }
    grant_write_acp: {
      common: false
      description: "Allows the named [grantee][urls.aws_s3_grantee] to write the created objects' ACL."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be","person@email.com","http://acs.amazonaws.com/groups/global/AllUsers"]
      }
    }
    key_prefix: {
      common: true
      description: "A prefix to apply to all object key names. This should be used to partition your objects, and it's important to end this value with a `/` if you want this to be the root S3 \"folder\"."
      required: false
      warnings: []
      type: string: {
        default: "date=%F/"
        examples: ["date=%F/","date=%F/hour=%H/","year=%Y/month=%m/day=%d/","application_id={{ application_id }}/date=%F/"]
      }
    }
    server_side_encryption: {
      common: false
      description: "The server-side encryption algorithm used when storing these objects."
      required: false
      warnings: []
      type: string: {
        default: null
        enum: {
          AES256: "256-bit Advanced Encryption Standard"
          aws:kms: "AWS managed key encryption"
        }
      }
    }
    ssekms_key_id: {
      common: false
      description: "If `server_side_encryption` has the value `\"aws.kms\"`, this specifies the ID of the AWS Key Management Service (AWS KMS) symmetrical customer managed customer master key (CMK) that will used for the created objects. If not specified, Amazon S3 uses the AWS managed CMK in AWS to protect the data."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["abcd1234"]
      }
    }
    storage_class: {
      common: false
      description: "The storage class for the created objects. See [the S3 Storage Classes](https://docs.aws.amazon.com/AmazonS3/latest/dev/storage-class-intro.html) for more details."
      required: false
      warnings: []
      type: string: {
        default: null
        enum: {
          STANDARD: "The default storage class. If you don't specify the storage class when you upload an object, Amazon S3 assigns the STANDARD storage class."
          REDUCED_REDUNDANCY: "Designed for noncritical, reproducible data that can be stored with less redundancy than the STANDARD storage class. AWS recommends that you not use this storage class. The STANDARD storage class is more cost effective. "
          INTELLIGENT_TIERING: "Stores objects in two access tiers: one tier that is optimized for frequent access and another lower-cost tier that is optimized for infrequently accessed data."
          STANDARD_IA: "Amazon S3 stores the object data redundantly across multiple geographically separated Availability Zones (similar to the STANDARD storage class)."
          ONEZONE_IA: "Amazon S3 stores the object data in only one Availability Zone."
          GLACIER: "Use for archives where portions of the data might need to be retrieved in minutes."
          DEEP_ARCHIVE: "Use for archiving data that rarely needs to be accessed."
        }
      }
    }
    tags: {
      common: false
      description: "The tag-set for the object."
      required: false
      warnings: []
      type: object: {
        examples: [{"Tag1":"Value1"}]
        options: {}
      }
    }
  }
}

