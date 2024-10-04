package metadata

base: components: sources: journald: configuration: {
	acknowledgements: {
		deprecated: true
		description: """
			Controls how acknowledgements are handled by this source.

			This setting is **deprecated** in favor of enabling `acknowledgements` at the [global][global_acks] or sink level.

			Enabling or disabling acknowledgements at the source level has **no effect** on acknowledgement behavior.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
			[e2e_acks]: https://vector.dev/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: "Whether or not end-to-end acknowledgements are enabled for this source."
			required:    false
			type: bool: {}
		}
	}
	batch_size: {
		description: """
			The systemd journal is read in batches, and a checkpoint is set at the end of each batch.

			This option limits the size of the batch.
			"""
		required: false
		type: uint: {
			default: 16
			unit:    "events"
		}
	}
	current_boot_only: {
		description: "Only include entries that occurred after the current boot of the system."
		required:    false
		type: bool: default: true
	}
	data_dir: {
		description: """
			The directory used to persist file checkpoint positions.

			By default, the [global `data_dir` option][global_data_dir] is used.
			Make sure the running user has write permissions to this directory.

			If this directory is specified, then Vector will attempt to create it.

			[global_data_dir]: https://vector.dev/docs/reference/configuration/global-options/#data_dir
			"""
		required: false
		type: string: examples: ["/var/lib/vector"]
	}
	emit_cursor: {
		description: """
			Whether to emit the [__CURSOR field][cursor]. See also [sd_journal_get_cursor][get_cursor].

			[cursor]: https://www.freedesktop.org/software/systemd/man/latest/systemd.journal-fields.html#Address%20Fields
			[get_cursor]: https://www.freedesktop.org/software/systemd/man/latest/sd_journal_get_cursor.html
			"""
		required: false
		type: bool: default: false
	}
	exclude_matches: {
		description: """
			A list of sets of field/value pairs that, if any are present in a journal entry,
			excludes the entry from this source.

			If `exclude_units` is specified, it is merged into this list.
			"""
		required: false
		type: object: {
			examples: [{
				"_SYSTEMD_UNIT": ["sshd.service", "ntpd.service"]
				"_TRANSPORT": ["kernel"]
			}]
			options: "*": {
				description: "The set of field values to match in journal entries that are to be excluded."
				required:    true
				type: array: items: type: string: {}
			}
		}
	}
	exclude_units: {
		description: """
			A list of unit names to exclude from monitoring.

			Unit names lacking a `.` have `.service` appended to make them a valid service unit
			name.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["badservice", "sysinit.target"]
		}
	}
	extra_args: {
		description: """
			A list of extra command line arguments to pass to `journalctl`.

			If specified, it is merged to the command line arguments as-is.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["--merge"]
		}
	}
	include_matches: {
		description: """
			A list of sets of field/value pairs to monitor.

			If empty or not present, all journal fields are accepted.

			If `include_units` is specified, it is merged into this list.
			"""
		required: false
		type: object: {
			examples: [{
				"_SYSTEMD_UNIT": ["sshd.service", "ntpd.service"]
				"_TRANSPORT": ["kernel"]
			}]
			options: "*": {
				description: "The set of field values to match in journal entries that are to be included."
				required:    true
				type: array: items: type: string: {}
			}
		}
	}
	include_units: {
		description: """
			A list of unit names to monitor.

			If empty or not present, all units are accepted.

			Unit names lacking a `.` have `.service` appended to make them a valid service unit name.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["ntpd", "sysinit.target"]
		}
	}
	journal_directory: {
		description: """
			The full path of the journal directory.

			If not set, `journalctl` uses the default system journal path.
			"""
		required: false
		type: string: {}
	}
	journal_namespace: {
		description: """
			The [journal namespace][journal-namespace].

			This value is passed to `journalctl` through the [`--namespace` option][journalctl-namespace-option].
			If not set, `journalctl` uses the default namespace.

			[journal-namespace]: https://www.freedesktop.org/software/systemd/man/systemd-journald.service.html#Journal%20Namespaces
			[journalctl-namespace-option]: https://www.freedesktop.org/software/systemd/man/journalctl.html#--namespace=NAMESPACE
			"""
		required: false
		type: string: {}
	}
	journalctl_path: {
		description: """
			The full path of the `journalctl` executable.

			If not set, a search is done for the `journalctl` path.
			"""
		required: false
		type: string: {}
	}
	remap_priority: {
		deprecated:         true
		deprecated_message: "This option has been deprecated, use the `remap` transform and `to_syslog_level` function instead."
		description: """
			Enables remapping the `PRIORITY` field from an integer to string value.

			Has no effect unless the value of the field is already an integer.
			"""
		required: false
		type: bool: default: false
	}
	since_now: {
		description: "Only include entries that appended to the journal after the entries have been read."
		required:    false
		type: bool: default: false
	}
}
