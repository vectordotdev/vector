package metadata

components: sinks: postgres: {
	title: "PostgreSQL"

	classes: {
		commonly_used: false
		delivery:      "exactly_once"
		development:   "beta"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
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
				service: services.postgres
				interface: {
					socket: {
						direction: "outgoing"
						protocols: ["tcp", "unix"]
						ssl: "optional"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		warnings: [
			"""
			[PostgreSQL's default values](\(urls.postgresql_default_values)) defined in the destination table
			are not supported. If the ingested event is missing a field which is present as a table column,
			a `null` value will be inserted for that record even if that column has a default value defined.
			This is a limitation of the `jsonb_populate_recordset` function of PostgreSQL.

			As a workaround, you can add a `NOT NULL` constraint to the column, so when inserting an event which is missing that field
			a `NOT NULL` constraint violation would be raised, and define a [constraint trigger](\(urls.postgresql_constraint_trigger))
			to catch the exception and set the desired default value.
			""",
		]
		notices: []
	}

	configuration: generated.components.sinks.postgres.configuration

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
		traces: true
	}

	how_it_works: {
		inserting_events_into_postgres: {
			title: "Inserting events into PostgreSQL"
			body:  """

				In order to insert data into a PostgreSQL table, you must first create a table that matches
				the json serialization of your event data. Note that this sink accepts `log`, `metric`, and `trace` events
				and the inserting behavior will be the same for all of them.

				For example, if your event is a log whose JSON serialization would have the following structure:
				```json
				{
				    "host": "localhost",
				    "message": "239.215.85.26 - AmbientTech [04/Mar/2025:15:09:25 +0100] \"DELETE /observability/metrics/production HTTP/1.0\" 300 37142",
				    "service": "vector",
				    "source_type": "demo_logs",
				    "timestamp": "2025-03-04T14:09:25.883572054Z"
				}
				```
				And you want to store all those fields, the table should be created as follows:
				```sql
				CREATE TABLE logs (
					host TEXT,
					message TEXT,
					service TEXT,
					source_type TEXT,
					timestamp TIMESTAMPTZ
				);
				```
				Note that not all fields must be declared in the table, only the ones you want to store. If a field is not present in the table
				but it is present in the event, it will be ignored.

				When inserting the event into the table, PostgreSQL will do a best-effort job of converting the JSON serialized
				event to the correct PostgreSQL data types.
				The semantics of the insertion will follow the `jsonb_populate_record` function of PostgresSQL,
				see [PostgreSQL documentation](\(urls.postgresql_json_functions)) about that function
				for more details about the inserting behavior.
				The correspondence between Vector types and PostgreSQL types can be found
				in the [`sqlx` crate's documentation](\(urls.postgresql_sqlx_correspondence))

				#### Practical example

				Spin up a PostgreSQL instance with Docker:
				```shell
				docker run -d --name postgres -e POSTGRES_PASSWORD=password123 -p 5432:5432 postgres
				```

				Create the following PostgreSQL table inside the `test` database:
				```sql
				CREATE TABLE logs (
					message TEXT,
					payload JSONB,
					timestamp TIMESTAMPTZ
				);
				```

				And the following Vector configuration:
				```yaml
				sources:
				  demo_logs:
				    type: demo_logs
				    format: apache_common
				transforms:
				  payload:
				    type: remap
				    inputs:
				      - demo_logs
				    source: |
				      .payload = .
				sinks:
				  postgres:
				    type: postgres
				    inputs:
				      - payload
				    endpoint: postgres://postgres:password123@localhost/test
				    table: logs
				```
				Then, you can see those log events ingested in the `logs` table.

				#### Composite Types

				When using PostgreSQL [composite types](\(urls.postgresql_composite_types)), the sink will attempt to insert the event data into
				the composite type, following its structure.

				Using the previous example, if you want to store the `payload` column as a composite type instead of `JSONB`,
				you should create the following composite type:
				```sql
				CREATE TYPE payload_type AS (
					host TEXT,
					message TEXT,
					service TEXT,
					source_type TEXT,
					timestamp TIMESTAMPTZ
				);
				```

				And the table should be created as follows:
				```sql
				CREATE TABLE logs (
					message TEXT,
					payload payload_type,
					timestamp TIMESTAMPTZ
				);
				```

				Then, you can see those log events ingested in the `logs` table and the `payload` column can be
				treated as a regular PostgreSQL composite type.

				#### Ingesting metrics

				When ingesting metrics, the sink will behave exactly the same as when ingesting logs. You must declare
				the table with the same fields as the JSON serialization of the metric event.

				For example, in order to ingest Vector's internal events, and only take into account `counter`, `gauge`, and `aggregated_histogram` metric data,
				you should create the following table:

				```sql
				create table metrics(
					name TEXT,
				    namespace TEXT,
					tags JSONB,
				 	timestamp TIMESTAMPTZ,
					kind TEXT,
					counter JSONB,
					gauge JSONB,
				 	aggregated_histogram JSONB
				);
				```

				And with this Vector configuration:
				```yaml
				sources:
				  internal_metrics:
				    type: internal_metrics
				sinks:
				  postgres:
				    type: postgres
				    inputs:
				      - internal_metrics
				    endpoint: postgres://postgres:password123@localhost/test
				    table: metrics
				```
				You can see those metric events ingested into the `metrics` table.
				"""
		}
	}
}
