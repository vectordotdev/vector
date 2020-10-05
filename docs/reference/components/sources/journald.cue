package metadata

components: sources: journald: {
  title: "#{component.title}"
  short_description: "Ingests data through [Systemd's][urls.systemd] [Journald][urls.journald] utility and outputs log events."
  long_description: "[Journald][urls.journald] is a utility for accessing log data across a variety of system services. It was introduce with [Systemd][urls.systemd] to help system administrator collect, access, and route log data."

  _features: {
    checkpoint: enabled: true
    multiline: enabled: false
    tls: enabled: false
  }

  classes: {
    commonly_used: true
    deployment_roles: ["daemon"]
    function: "collect"
  }

  statuses: {
    delivery: "at_least_once"
    development: "beta"
  }

  support: {

    platforms: {
      "aarch64-unknown-linux-gnu": true
      "aarch64-unknown-linux-musl": true
      "x86_64-apple-darwin": false
      "x86_64-pc-windows-msv": false
      "x86_64-unknown-linux-gnu": true
      "x86_64-unknown-linux-musl": true
    }

    requirements: [
      """
      The `journalctl` binary is required, this is the interface Vector uses to retrieve JournalD logs. See the ["Communication Strategy" section][docs.sources.journald#communication-strategy] for more info.
      """,
      """
      The Vector user must be part of the `systemd-journal` group in order to execute the `journalctl` binary. See the ["User Permissions" section][docs.sources.journald#user-permissions] for more info.
      """,
    ]
    warnings: []
  }

  configuration: {
    batch_size: {
      common: false
      description: "The systemd journal is read in batches, and a checkpoint is set at the end of each batch. This option limits the size of the batch."
      required: false
      warnings: []
      type: uint: {
        default: 16
        unit: null
      }
    }
    current_boot_only: {
      common: true
      description: "Include only entries from the current boot."
      required: false
      warnings: []
      type: bool: default: true
    }
    data_dir: {
      common: false
      description: "The directory used to persist the journal checkpoint position. By default, the global `data_dir` is used. Please make sure the Vector project has write permissions to this dir."
      required: false
      warnings: []
      type: string: {
        default: null
        examples: ["/var/lib/vector"]
      }
    }
    exclude_units: {
      common: true
      description: "The list of units names to exclude from monitoring. Unit names lacking a `\".\"` will have `\".service\"` appended to make them a valid service unit name."
      required: false
      warnings: []
      type: "[string]": {
        default: []
        examples: [["badservice","sysinit.target"]]
      }
    }
    include_units: {
      common: true
      description: "The list of units names to monitor. If empty or not present, all units are accepted. Unit names lacking a `\".\"` will have `\".service\"` appended to make them a valid service unit name."
      required: false
      warnings: []
      type: "[string]": {
        default: []
        examples: [["ntpd","sysinit.target"]]
      }
    }
    journalctl_path: {
      common: false
      description: "The full path of the `journalctl` executable. If not set, Vector will search the path for `journalctl`."
      required: false
      warnings: []
      type: string: {
        default: "journalctl"
        examples: ["/usr/local/bin/journalctl"]
      }
    }
    remap_priority: {
      common: false
      description: "If the record from journald contains a `PRIORITY` field, it will be remapped into the equivalent syslog priority level name using the standard (abbreviated) all-capitals names such as `EMERG` or `ERR`."
      required: false
      warnings: []
      type: bool: default: false
    }
  }
}
