package metadata

components: sinks: duckdb: {
	title: "DuckDB"

	classes: {
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10_000_000
				timeout_secs: 1.0
			}
			request: {
				enabled: true
				headers: false
			}
			compression: enabled: false
			encoding: enabled:    false
			tls: enabled:         false
			to: {
				service: services.duckdb
				interface: {
					file_system: {
						directory: "configured DuckDB database path"
					}
				}
			}
		}
	}

	support: {
		requirements: [
			"""
				The destination table must already exist before Vector starts. Vector reads the
				DuckDB table schema at startup and encodes each batch to match that schema.
				""",
		]
		warnings: []
		notices: [
			"""
				The DuckDB sink is not enabled by Vector's default `sinks` feature set. Builds
				that include this sink must enable the `sinks-duckdb` feature. The current
				implementation uses DuckDB's bundled library through `duckdb-rs`, which adds a
				larger native build dependency than most sinks.
				""",
		]
	}

	configuration: generated.components.sinks.duckdb.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		table_schema: {
			title: "Table Schema"
			body: """
				The DuckDB sink writes log events to an existing DuckDB table. Vector fetches
				the configured table schema from `information_schema.columns` during startup,
				converts that schema to Arrow, and encodes each event batch as an Arrow
				`RecordBatch`. The batch is then appended using DuckDB's appender API.

				For example, given log events with `id`, `host`, `message`, and `timestamp`
				fields, create a table like:

				```sql
				CREATE TABLE events (
				    id INTEGER NOT NULL,
				    host VARCHAR,
				    message VARCHAR,
				    timestamp TIMESTAMP
				);
				```

				Then configure Vector:

				```toml
				[sinks.duckdb]
				type = "duckdb"
				inputs = ["my_source"]
				endpoint = "duckdb:///var/lib/vector/events.duckdb"
				table = "events"
				```

				Columns in the DuckDB table select the event fields that are stored. Event
				fields not present in the destination table are ignored by the Arrow encoder.
				If an event is missing a non-nullable table column, encoding fails for the
				batch and the events are rejected.
				"""
		}

		database: {
			title: "Database/Schema"
			body: """
				By default, the sink writes to DuckDB's `main` database/schema. Set the
				`database` option when the target table lives in another schema:

				```toml
				[sinks.duckdb]
				type = "duckdb"
				endpoint = "duckdb:///var/lib/vector/events.duckdb"
				database = "analytics"
				table = "events"
				```
				"""
		}

		type_mappings: {
			title: "Type Mappings"
			body:  """
				Vector maps supported [DuckDB data types](\(urls.duckdb_data_types)) to Arrow
				types before encoding batches.

				Supported types include booleans, signed and unsigned integers, floating point
				numbers, `VARCHAR`/`TEXT`, `BLOB`, `DATE`, `TIME`, `TIMESTAMP`, and
				`DECIMAL`/`NUMERIC`.

				Unsupported destination column types fail schema resolution during sink build
				or healthcheck. Complex DuckDB types such as `STRUCT`, `LIST`, `MAP`, and
				`UNION` are not yet supported.
				"""
		}

		batching: {
			title: "Batching and Transactions"
			body: """
				Events are batched using the standard sink `batch` options. Each batch is
				encoded and appended inside a DuckDB transaction, so a successful batch commit
				makes all events in that batch visible together. DuckDB access is performed on
				blocking worker threads because DuckDB's Rust API is synchronous.
				"""
		}
	}
}
