package metadata

_schemaDefinitions: "derived::a92b0a9eb567f8513c87e07b": object: {
	examples: [{
		excludes: [
			"dm-*"
		]
		includes: [
			"sda"
		]
	}]
	options: {
		excludes: {
			description: """
				Any patterns which should be excluded.

				The patterns are matched using globbing.
				"""
			required: false
			type: array: items: type: string: {}
		}
		includes: {
			description: """
				Any patterns which should be included.

				The patterns are matched using globbing.
				"""
			required: false
			type: array: {
				default: [
					"*"
				]
				items: type: string: {}
			}
		}
	}
}
