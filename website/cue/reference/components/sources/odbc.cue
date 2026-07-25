package metadata

components: sources: odbc: {
	title: "ODBC"

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		deployment_roles: ["daemon", "sidecar", "aggregator"]
		development:   "beta"
		egress_method: "batch"
		stateful:      true
	}

	features: {
		auto_generated:   true
		acknowledgements: false
		collect: {
			checkpoint: enabled: false
			from: {
				service: services.odbc
			}
		}
		multiline: enabled: false
	}

	support: {
		requirements: [
			"""
				Only included in official 64-bit glibc Linux builds
				(`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`) and the
				official Windows archive (`x86_64-pc-windows-msvc`). It is not
				included in musl builds (Alpine, distroless-static,
				`*-unknown-linux-musl`), 32-bit ARM GNU builds
				(`armv7-unknown-linux-gnueabihf`, `arm-unknown-linux-gnueabi`), or
				official macOS archives (`aarch64-apple-darwin`). For those
				targets, use a custom build with `sources-odbc`.
				""",
			"""
				Linux glibc builds link the unixODBC driver manager (`libodbc`).
				Official Debian and distroless-libc images include it; `.deb`
				packages depend on `libodbc2` or `libodbc1`. Windows builds use the
				system ODBC Driver Manager. A database ODBC driver must still be
				installed and configured separately.
				""",
		]
		warnings: [
			"""
				When `last_run_metadata_path` is set, the query result is buffered and the
				final-row tracking checkpoint is validated and saved before any batches are
				sent. If the checkpoint save succeeds but downstream delivery then fails (or
				Vector restarts before delivery is complete), those rows may be skipped on the
				next run (at-most-once). If the checkpoint save itself fails, previous tracking
				values are kept and the next scheduled run may re-emit the same rows. This
				source does not provide acknowledgements.
				""",
			"""
				Setting `login_timeout` or `statement_timeout` to `0` disables that bound.
				Shutdown still waits for any in-flight ODBC connect or execute, so `0` can
				delay Vector exit until the driver returns. Prefer positive timeouts.
				""",
		]
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: generated.components.sources.odbc.configuration

	output: {
		logs: record: {
			description: """
				A single row returned by the ODBC query. Each column becomes a
				top-level log field and retains its Vector typed value when possible
				(for example naive timestamps via `odbc_default_timezone`, integers,
				booleans, and floats). Offset-bearing SQL or RFC3339 timestamp text
				is emitted as bytes so tracking parameters can reuse the exact ODBC
				text. Other columns that cannot be represented as a native Vector
				type are also emitted as bytes.
				"""
			fields: {
				"*": {
					common:      false
					description: "A column from the query result set."
					required:    false
					type: "*": {}
				}
				timestamp: fields._current_timestamp
			}
		}
	}

	how_it_works: {
		requirement: {
			title: "Requirement for unixODBC"
			body: """
				To connect to a database and execute queries via ODBC, you must have the unixODBC
				driver manager available, then install and configure the appropriate ODBC driver.
				See Requirements above for which official Vector builds include this source and
				the driver manager.

				For example, on Debian-based Linux:
				```bash
				# apt-get install unixodbc odbcinst odbc-mariadb
				```

				You can use the `odbcinst -j` command to check the installation path and configuration files for unixODBC.
				```bash
				$ odbcinst -j
				unixODBC 2.3.12
				DRIVERS............: /etc/odbcinst.ini
				SYSTEM DATA SOURCES: /etc/odbc.ini
				FILE DATA SOURCES..: /etc/ODBCDataSources
				USER DATA SOURCES..: /root/.odbc.ini
				SQLULEN Size.......: 8
				SQLLEN Size........: 8
				SQLSETPOSIROW Size.: 8
				```

				Review the `/etc/odbcinst.ini` file in the output to ensure the ODBC driver is properly configured.
				If you installed the ODBC driver via a package manager, it is usually configured automatically.
				When you install the `odbc-mariadb` package, the `odbcinst.ini` file will be configured as follows:
				```bash
				$ cat /etc/odbcinst.ini

				[MariaDB Unicode]
				Driver=libmaodbc.so
				Description=MariaDB Connector/ODBC(Unicode)
				Threading=0
				UsageCount=1
				```
				"""
		}

		examples: {
			title: "Example ODBC Source Configuration"
			body: """
					This section walks through a simple example of configuring an ODBC data source and scheduling it.
				"""
			sub_sections: [
				{
					title: "Step 1: Configure Test Data"
					body: """
						Given the following MariaDB table and sample data:

						```sql
						create table odbc_table
						(
						  id int auto_increment primary key,
						  name varchar(255) null,
						  `datetime` datetime null
						);

						INSERT INTO odbc_table (name, datetime) VALUES
						('test1', now()),
						('test2', now()),
						('test3', now()),
						('test4', now()),
						('test5', now());
						```
						"""
				},
				{
					title: "Step 2: Configure ODBC Source"
					body: """
						The example below shows how to connect to a MariaDB database with the ODBC driver,
						run a query periodically, and send the results to Vector.
						Start by providing a database connection string.

						```yaml
						sources:
						  odbc:
						    type: odbc
						    connection_string: "driver={MariaDB Unicode};server=<your server>;port=<your port>;database=<your database>;uid=<your uid>;pwd=<your password>;"
						    statement: "SELECT * FROM odbc_table WHERE id > ? ORDER BY id ASC LIMIT 1;"
						    statement_init_params:
						      - name: id
						        value: "0"
						    schedule: "*/5 * * * * *"
						    schedule_timezone: UTC
						    last_run_metadata_path: /path/to/odbc_tracking.json
						    tracking_columns:
						      - id

						sinks:
						  console:
						    type: console
						    inputs:
						      - odbc
						    encoding:
						      codec: json
						```

						Every five seconds, the source emits one log event per result row.
						Column values keep their Vector types when possible (for example
						naive `datetime` values as timestamps via `odbc_default_timezone`,
						and `id` as an integer). Offset-bearing SQL or RFC3339 timestamp
						text is kept as bytes so tracking parameters round-trip the exact
						ODBC text. When a sink encodes events as JSON, the output looks
						similar to the following.

						```json
						{"datetime":"2025-04-28T01:20:04Z","id":1,"name":"test1","source_type":"odbc","timestamp":"2025-04-28T01:50:45.075484Z"}
						{"datetime":"2025-04-28T01:20:04Z","id":2,"name":"test2","source_type":"odbc","timestamp":"2025-04-28T01:50:50.017276Z"}
						{"datetime":"2025-04-28T01:20:04Z","id":3,"name":"test3","source_type":"odbc","timestamp":"2025-04-28T01:50:55.016432Z"}
						{"datetime":"2025-04-28T01:20:04Z","id":4,"name":"test4","source_type":"odbc","timestamp":"2025-04-28T01:51:00.016328Z"}
						{"datetime":"2025-04-28T01:20:04Z","id":5,"name":"test5","source_type":"odbc","timestamp":"2025-04-28T01:51:05.010063Z"}
						```
						"""
				},
			]
		}

		timestamp_mapping: {
			title: "Timestamp mapping"
			body: """
				Naive date/time text from the driver is parsed to a Vector timestamp
				using `odbc_default_timezone`. Timestamp text that already includes a
				zone, such as SQL-style `YYYY-MM-DD HH:MM:SS+02:00` or RFC3339 values
				like `2025-04-28T01:20:04Z` / `2025-04-28T01:20:04+02:00`, is preserved
				as bytes. That keeps tracking-column round-trips faithful to the
				original ODBC text instead of rebinding a naive local datetime that can
				skip or replay rows when the database offset differs from
				`odbc_default_timezone`.
				"""
		}

		check_license: {
			title: "Check ODBC Driver License"
			body:  """
        Review the license information on [the official unixODBC website](\(urls.unixodbc)).

        Because ODBC drivers are supplied by various vendors, each with different license terms,
        be sure to review and comply with the terms for the driver you plan to use.
        """
		}
	}
}
