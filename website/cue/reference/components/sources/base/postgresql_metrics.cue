package metadata

base: components: sources: postgresql_metrics: configuration: {
	endpoints: {
		description: """
			A list of PostgreSQL instances to scrape.

			Each endpoint must be in the [Connection URI
			format](https://www.postgresql.org/docs/current/libpq-connect.html#id-1.7.3.8.3.6).
			"""
		required: true
		type: array: items: type: string: examples: ["postgresql://postgres:vector@localhost:5432/postgres"]
	}
	exclude_databases: {
		description: """
			A list of databases to match (by using [POSIX Regular
			Expressions](https://www.postgresql.org/docs/current/functions-matching.html#FUNCTIONS-POSIX-REGEXP)) against
			the `datname` column for which you don’t want to collect metrics from.

			Specifying `""` includes metrics where `datname` is `NULL`.

			This can be used in conjunction with `include_databases`.
			"""
		required: false
		type: array: items: type: string: examples: ["^postgres$", "^template.*"]
	}
	include_databases: {
		description: """
			A list of databases to match (by using [POSIX Regular
			Expressions](https://www.postgresql.org/docs/current/functions-matching.html#FUNCTIONS-POSIX-REGEXP)) against
			the `datname` column for which you want to collect metrics from.

			If not set, metrics are collected from all databases. Specifying `""` includes metrics where `datname` is
			`NULL`.

			This can be used in conjunction with `exclude_databases`.
			"""
		required: false
		type: array: items: type: string: examples: ["^postgres$", "^vector$", "^foo"]
	}
	namespace: {
		description: "Overrides the default namespace for the metrics emitted by the source."
		required:    false
		type: string: default: "postgresql"
	}
	scrape_interval_secs: {
		description: "The interval between scrapes."
		required:    false
		type: uint: {
			default: 15
			unit:    "seconds"
		}
	}
	tls: {
		description: "Configuration of TLS when connecting to PostgreSQL."
		required:    false
		type: object: options: ca_file: {
			description: """
				Absolute path to an additional CA certificate file.

				The certificate must be in the DER or PEM (X.509) format.
				"""
			required: true
			type: string: examples: ["certs/ca.pem"]
		}
	}
}
