package metadata

components: transforms: swimlanes: {
	title:             "Swimlanes"
	short_description: "Accepts log events and allows you to route events across parallel streams using logical filters."
	long_description:  "Accepts log events and allows you to route events across parallel streams using logical filters."

	_features: {
		checkpoint: enabled: false
		multiline: enabled:  false
		tls: enabled:        false
	}

	classes: {
		commonly_used: false
		function:      "route"
	}

	statuses: {
		development: "beta"
	}

	support: {
		input_types: ["log"]

		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
	}

	configuration: {
		lanes: {
			description: "A table of swimlane identifiers to logical conditions representing the filter of the swimlane. Each swimlane can then be referenced as an input by other components with the name `<transform_name>.<swimlane_id>`."
			required:    true
			warnings: []
			type: object: configuration._conditions
		}
	}

  examples: log: [
    {
      title: "Split by log level"
      configuration: {
        lanes: {
          debug: "level.eq": "debug"
          info: "level.eq": "info"
          warn: "level.eq": "warn"
          error: "level.eq": "error"
        }
      }
      input: {
        level: "info"
      }
      output: {
        level: "info"
      }
    }
  ]
}
