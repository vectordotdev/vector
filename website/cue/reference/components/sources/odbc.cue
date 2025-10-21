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
			description: "The ODBC query result records."
			fields: {
				message: {
					description: "The ODBC query results record in JSON format."
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

		check_license: {
			title: "Check ODBC Driver License"
			body:  """
        Check the license on [the official unixODBC website](\(urls.unixodbc)).

        Also, ODBC drivers are provided by various vendors, each with different license terms.
        Be sure to review and comply with the license terms of the ODBC driver you intend to use.
        """
		}
	}
}
