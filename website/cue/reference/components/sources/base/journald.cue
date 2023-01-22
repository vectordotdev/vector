package metadata

base: components: sources: journald: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled by this source.

			This setting is **deprecated** in favor of enabling `acknowledgements` at the [global][global_acks] or sink level. Enabling or disabling acknowledgements at the source level has **no effect** on acknowledgement behavior.

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
		description: "The `systemd` journal is read in batches, and a checkpoint is set at the end of each batch. This option limits the size of the batch."
		required:    false
		type: uint: {}
	}
	current_boot_only: {
		description: "Only include entries that occurred after the current boot of the system."
		required:    false
		type: bool: {}
	}
	data_dir: {
		description: """
			The directory used to persist file checkpoint positions.

			By default, the global `data_dir` option is used. Make sure the running user has write permissions to this directory.
			"""
		required: false
		type: string: {}
	}
	exclude_matches: {
		description: """
			A list of sets of field/value pairs that, if any are present in a journal entry, will cause the entry to be excluded from this source.

			If `exclude_units` is specified, it will be merged into this list.
			"""
		required: false
		type: object: options: "*": {
			description: "A field/value pair."
			required:    true
			type: array: items: type: string: {}
		}
	}
	exclude_units: {
		description: """
			A list of unit names to exclude from monitoring.

			Unit names lacking a "." will have ".service" appended to make them a valid service unit name.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: {}
		}
	}
	include_matches: {
		description: """
			A list of sets of field/value pairs to monitor.

			If empty or not present, all journal fields are accepted. If `include_units` is specified, it will be merged into this list.
			"""
		required: false
		type: object: options: "*": {
			description: "A field/value pair."
			required:    true
			type: array: items: type: string: {}
		}
	}
	include_units: {
		description: """
			A list of unit names to monitor.

			If empty or not present, all units are accepted. Unit names lacking a "." will have ".service" appended to make them a valid service unit name.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: {}
		}
	}
	journal_directory: {
		description: """
			The full path of the journal directory.

			If not set, `journalctl` will use the default system journal paths.
			"""
		required: false
		type: string: {}
	}
	journalctl_path: {
		description: """
			The full path of the `journalctl` executable.

			If not set, a search is done for the journalctl` path.
			"""
		required: false
		type: string: {}
	}
	remap_priority: {
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
		type: bool: {}
	}
	units: {
		description: """
			The list of unit names to monitor.

			If empty or not present, all units are accepted. Unit names lacking a "." will have ".service" appended to make them a valid service unit name.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: {}
		}
	}
}
