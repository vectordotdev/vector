package metadata

import (
  "strings"
)

components: sources: vector: {
  title: "#{component.title}"
  short_description: strings.ToTitle(classes.function) + " log and metric another upstream [`vector` sink][docs.sinks.vector]"
  description: null

  _features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: {
      enabled: true
      can_enable: true
      can_verify_certificate: true
      can_verify_hostname: false
      enabled_default: false
    }
  }

  classes: {
    commonly_used: false
    deployment_roles: ["service"]
    function: "receive"
  }

  statuses: {
    delivery: "best_effort"
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
    address: {
      common: true
      description: "The TCP address to listen for connections on, or `systemd#N to use the Nth socket passed by systemd socket activation. If an address is used it _must_ include a port.\n"
      required: true
        type: string: {
          examples: ["0.0.0.0:9000","systemd","systemd#1"]
        }
    }
    shutdown_timeout_secs: {
      common: false
      description: "The timeout before a connection is forcefully closed during shutdown."
      required: false
        type: uint: {
          default: 30
          unit: "seconds"
        }
    }
    tls: {
      common: false
      description: "Configures the TLS options for connections from this source."
      required: false
        type: object: {
          default: null
          examples: []
          options: {
            type: string: {
              default: null
              examples: ["/path/to/certificate_authority.crt"]
            }
            type: string: {
              default: null
              examples: ["/path/to/host_certificate.crt"]
            }
            type: bool: default: false
            type: string: {
              default: null
              examples: ["/path/to/host_certificate.key"]
            }
            type: string: {
              default: null
              examples: ["${KEY_PASS_ENV_VAR}","PassWord1"]
            }
            type: bool: default: false
          }
        }
    }
  }
}
