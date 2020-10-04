package metadata

components: sinks: gcp_cloud_storage: {
  title: "#{component.title}"
  short_description: "Batches log events to [Google Cloud Platform's Cloud Storage service](https://cloud.google.com/storage) via the [XML Interface](https://cloud.google.com/storage/docs/xml-api/overview)."
  long_description: "[Google Cloud Storage][urls.gcp_cloud_storage] is a RESTful online file storage web service for storing and accessing data on Google Cloud Platform infrastructure. The service combines the performance and scalability of Google's cloud with advanced security and sharing capabilities. This makes it a prime candidate for log data."

  _features: {
    batch: {
      enabled: true
      common: false,
      max_bytes: 10485760,
      max_events: null,
      timeout_secs: 300
    }
    buffer: enabled: true
    checkpoint: enabled: false
    compression: {
      enabled: true
      default: "none"
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
      in_flight_limit: 25,
      rate_limit_duration_secs: 1,
      rate_limit_num: 1000,
      retry_initial_backoff_secs: 1,
      retry_max_duration_secs: 10,
      timeout_secs: 60
    }
    tls: {
      enabled: true
      can_enable: false
      can_verify_certificate: true
      can_verify_hostname: true
      enabled_default: false
    }
  }

  classes: {
    commonly_used: true
    function: "transmit"
    service_providers: ["GCP"]
  }

  statuses: {
    delivery: "at_least_once"
    development: "beta"
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
      description: "Predefined ACL to apply to the created objects. For more information, see [Predefined ACLs][urls.gcs_predefined_acl]. If this is not set, GCS will apply a default ACL when the object is created."
      groups: []
      required: false
      warnings: []
        type: string: {
          default: null
          enum: {
            authenticatedRead: "Gives the bucket or object owner OWNER permission, and gives all authenticated Google account holders READER permission."
            bucketOwnerFullControl: "Gives the object and bucket owners OWNER permission."
            bucketOwnerRead: "Gives the object owner OWNER permission, and gives the bucket owner READER permission."
            private: "Gives the bucket or object owner OWNER permission for a bucket or object."
            projectPrivate: "Gives permission to the project team based on their roles. Anyone who is part of the team has READER permission. Project owners and project editors have OWNER permission. This the default."
            publicRead: "Gives the bucket or object owner OWNER permission, and gives all users, both authenticated and anonymous, READER permission. When you apply this to an object, anyone on the Internet can read the object without authenticating."
          }
        }
    }
    bucket: {
      common: false
      description: "The GCS bucket name."
      groups: []
      required: true
      warnings: []
        type: string: {
          examples: ["my-bucket"]
        }
    }
    compression: {
      common: true
      description: "The compression strategy used to compress the encoded event data before transmission."
      groups: []
      required: false
      warnings: []
        type: string: {
          default: "none"
          enum: {
            none: "No compression."
            gzip: "[Gzip][urls.gzip] standard DEFLATE compression."
          }
        }
    }
    credentials_path: {
      common: true
      description: "The filename for a Google Cloud service account credentials JSON file used to authenticate access to the Cloud Storage API. If this is unset, Vector checks the `GOOGLE_APPLICATION_CREDENTIALS` environment variable for a filename.\n\nIf no filename is named, Vector will attempt to fetch an instance service account for the compute instance the program is running on. If Vector is not running on a GCE instance, you must define a credentials file as above."
      groups: []
      required: false
      warnings: []
        type: string: {
          default: null
          examples: ["/path/to/credentials.json"]
        }
    }
    filename_append_uuid: {
      common: false
      description: "Whether or not to append a UUID v4 token to the end of the file. This ensures there are no name collisions high volume use cases."
      groups: []
      required: false
      warnings: []
        type: bool: default: true
    }
    filename_extension: {
      common: false
      description: "The filename extension to use in the object name."
      groups: []
      required: false
      warnings: []
        type: string: {
          default: "log"
        }
    }
    filename_time_format: {
      common: false
      description: "The format of the resulting object file name. [`strftime` specifiers][urls.strptime_specifiers] are supported."
      groups: []
      required: false
      warnings: []
        type: string: {
          default: "%s"
        }
    }
    key_prefix: {
      common: true
      description: "A prefix to apply to all object key names. This should be used to partition your objects, and it's important to end this value with a `/` if you want this to be the root GCS \"folder\"."
      groups: []
      required: false
      warnings: []
        type: string: {
          default: "date=%F/"
          examples: ["date=%F/","date=%F/hour=%H/","year=%Y/month=%m/day=%d/","application_id={{ application_id }}/date=%F/"]
        }
    }
    metadata: {
      common: false
      description: "The set of metadata `key:value` pairs for the created objects. See the [GCS custom metadata][urls.gcs_custom_metadata] documentation for more details."
      groups: []
      required: false
      warnings: []
        type: string: {
          default: null
          examples: []
        }
    }
    storage_class: {
      common: false
      description: "The storage class for the created objects. See [the GCP storage classes][urls.gcs_storage_classes] for more details."
      groups: []
      required: false
      warnings: []
        type: string: {
          default: null
          enum: {
            STANDARD: "Standard Storage is best for data that is frequently accessed and/or stored for only brief periods of time. This is the default."
            NEARLINE: "Nearline Storage is a low-cost, highly durable storage service for storing infrequently accessed data."
            COLDLINE: "Coldline Storage is a very-low-cost, highly durable storage service for storing infrequently accessed data."
            ARCHIVE: "Archive Storage is the lowest-cost, highly durable storage service for data archiving, online backup, and disaster recovery."
          }
        }
    }
  }
}
