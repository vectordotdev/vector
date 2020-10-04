package metadata

components: sinks: console: {
  title: "#{component.title}"
  short_description: "Streams log and metric events to [standard output streams][urls.standard_streams], such as [STDOUT][urls.stdout] and [STDERR][urls.stderr]."
  long_description: "Streams log and metric events to [standard output streams][urls.standard_streams], such as [STDOUT][urls.stdout] and [STDERR][urls.stderr]."

  _features: {
    batch: enabled: false
    buffer: enabled: false
    checkpoint: enabled: false
    compression: enabled: false
    encoding: {
      enabled: true
      default: null
      ndjson: null
      text: null
    }
    multiline: enabled: false
    request: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: false
    function: "test"
    service_providers: []
  }

  statuses: {
    delivery: "at_least_once"
    development: "stable"
  }

  support: {
      input_types: ["log","metric"]

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
    target: {
      common: true
      description: "The [standard stream][urls.standard_streams] to write to."
      required: false
      warnings: []
      type: string: {
        default: "stdout"
        enum: {
          stdout: "Output will be written to [STDOUT][urls.stdout]"
          stderr: "Output will be written to [STDERR][urls.stderr]"
        }
      }
    }
  }
}

