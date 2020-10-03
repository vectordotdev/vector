package metadata

components: transforms: aws_ec2_metadata: {
  title: "#{component.title}"
  short_description: "Accepts log events and allows you to enrich logs with AWS EC2 instance metadata."
  description: "Accepts log events and allows you to enrich logs with AWS EC2 instance metadata."

  _features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    function: "enrich"
  }

  statuses: {
    development: "beta"
  }

  support: {
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
    endpoint: {
      common: true
      description: "Override the default EC2 Metadata endpoint."
      required: false
        type: string: {
          default: "http://169.254.169.254"
        }
    }
    fields: {
      common: true
      description: "A list of fields to include in each event."
      required: false
        type: "[string]": {
          default: ["instance-id","local-hostname","local-ipv4","public-hostname","public-ipv4","ami-id","availability-zone","vpc-id","subnet-id","region"]
        }
    }
    namespace: {
      common: true
      description: "Prepend a namespace to each field's key."
      required: false
        type: string: {
          default: ""
          examples: ["","ec2","aws.ec2"]
        }
    }
    refresh_interval_secs: {
      common: true
      description: "The interval in seconds at which the EC2 Metadata api will be called."
      required: false
        type: uint: {
          default: 10
        }
    }
  }
}