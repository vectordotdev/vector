package metadata

components: sources: odbc: {
	title: "ODBC"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "sidecar", "aggregator"]
		development:   "beta"
		egress_method: "batch"
		stateful:      true
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		collect: {
			checkpoint: enabled: false
			from: {
				service: services.odbc
			}
		}
		multiline: enabled: true
		encoding: enabled:  true
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: generated.components.sources.odbc.configuration

	output: {
		logs: record: {
			description: "Records returned by the ODBC query."
			fields: {
				message: {
					description: "The ODBC query result serialized as JSON."
					required:    true
					type: string: {
						examples: [
							"""
								[{"id":1,"name":"test1"}]
								""",
						]
					}
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

						```toml
						[sources.odbc]
						type = "odbc"
						connection_string = "driver={MariaDB Unicode};server=<your server>;port=<your port>;database=<your database>;uid=<your uid>;pwd=<your password>;"
						statement = "SELECT * FROM odbc_table WHERE id > ? LIMIT 1;"
						statement_init_params = { id = "0" }
						schedule = "*/5 * * * * *"
						schedule_timezone = "UTC"
						last_run_metadata_path = "/path/to/odbc_tracking.json"
						tracking_columns = ["id"]

						[sinks.console]
						type = "console"
						inputs = ["odbc"]
						encoding.codec = "json"
						```

						Every five seconds, the source produces output similar to the following.

						```json
						{"message":[{"datetime":"2025-04-28T01:20:04Z","id":1,"name":"test1"}],"timestamp":"2025-04-28T01:50:45.075484Z"}
						{"message":[{"datetime":"2025-04-28T01:20:04Z","id":2,"name":"test2"}],"timestamp":"2025-04-28T01:50:50.017276Z"}
						{"message":[{"datetime":"2025-04-28T01:20:04Z","id":3,"name":"test3"}],"timestamp":"2025-04-28T01:50:55.016432Z"}
						{"message":[{"datetime":"2025-04-28T01:20:04Z","id":4,"name":"test4"}],"timestamp":"2025-04-28T01:51:00.016328Z"}
						{"message":[{"datetime":"2025-04-28T01:20:04Z","id":5,"name":"test5"}],"timestamp":"2025-04-28T01:51:05.010063Z"}
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
