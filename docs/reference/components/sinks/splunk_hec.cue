package metadata

components: sinks: splunk_hec: {
  title: "Splunk HEC"
  short_description: "Batches log events to a [Splunk's HTTP Event Collector][urls.splunk_hec]."
  long_description: "The [Splunk HTTP Event Collector (HEC)][urls.splunk_hec] is a fast and efficient way to send data to Splunk Enterprise and Splunk Cloud. Notably, HEC enables you to send data over HTTP (or HTTPS) directly to Splunk Enterprise or Splunk Cloud from your application."

  _features: {
    batch: {
      enabled: true
      common: false,
      max_bytes: 1049000,
      max_events: null,
      timeout_secs: 1
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
      json: null
      ndjson: null
      text: null
    }
    healthcheck: enabled: true
    multiline: enabled: false
    request: {
      enabled: true
      common: false,
      in_flight_limit: 10,
      rate_limit_duration_secs: 1,
      rate_limit_num: 10,
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
    service_providers: ["Splunk"]
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
    host_key: {
      common: true
      description: "The name of the log field to be used as the hostname sent to Splunk HEC. This overrides the [global `host_key` option][docs.reference.global-options#host_key]."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["hostname"]
      }
    }
    index: {
      common: false
      description: "The name of the index where send the events to. If not specified, the default index is used.\n"
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["custom_index"]
      }
    }
    indexed_fields: {
      common: true
      description: "Fields to be [added to Splunk index][urls.splunk_hec_indexed_fields]."
      required: false
      warnings: []
      type: "[string]": {
        default: null
        examples: [["field1","field2"]]
      }
    }
    source: {
      common: false
      description: "The source of events sent to this sink. Typically the filename the logs originated from. If unset, the Splunk collector will set it.\n"
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["/var/log/syslog","UDP:514"]
      }
    }
    sourcetype: {
      common: false
      description: "The sourcetype of events sent to this sink. If unset, Splunk will default to httpevent.\n"
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["_json","httpevent"]
      }
    }
    token: {
      description: "Your Splunk HEC token."
      required: true
      warnings: []
      type: string: {
        examples: ["${SPLUNK_HEC_TOKEN}","A94A8FE5CCB19BA61C4C08"]
      }
    }
  }
}

