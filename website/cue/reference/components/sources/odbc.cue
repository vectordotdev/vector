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
		requirements: []
		warnings: [
			"""
				When `last_run_metadata_path` is set, tracking metadata is updated only after
				all result batches are converted and sent. If saving the checkpoint then fails,
				previous tracking values are kept and the next scheduled run may re-emit the
				same rows. If Vector restarts after a successful checkpoint write but before
				downstream delivery is fully acknowledged, rows can still be lost because this
				source does not provide acknowledgements.
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
				(for example timestamps, integers, booleans, and floats). Columns
				that cannot be represented as a native Vector type are emitted as
				bytes.
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
				To connect to a database and execute queries via ODBC, you must have the unixODBC package installed.
				First, use your package manager to install the `unixodbc` package.
				Then, install and configure the appropriate ODBC driver.

				For example, on Debian-based Linux, you can install the `unixodbc` and `odbc-mariadb` packages as follows:
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
						    statement: "SELECT * FROM odbc_table WHERE id > ? LIMIT 1;"
						    statement_init_params:
						      id: "0"
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
						`datetime` as a timestamp and `id` as an integer). When a sink
						encodes events as JSON, the output looks similar to the following.

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
