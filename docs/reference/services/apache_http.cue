package metadata

services: apache_http: {
	_config_path:   "/etc/apache2/httpd.conf"
	_endpoint_path: "/server-status"

	name:     "Apache HTTP server (HTTPD)"
	thing:    "an \(name)"
	url:      urls.apache
	versions: null

	setup: [
		{
			title: "Install Apache HTTP"
			description: """
				Install Apache HTTP by following their installation instructions.
				"""
			detour: url: urls.apache_install
		},
	]

	connect_to: {
		vector: metrics: {
			setup: [
				{
					title:       "Enable the Apache Status Module"
					description: """
						Enable the [Apache Status module](\(urls.apache_mod_status))
						in your Apache config.

						```text file="\(_config_path)"
						# ...

						<Location "\(_endpoint_path)">
						    SetHandler server-status
						    Require host example.com
						</Location>

						# ...
						```
						"""
				},
				{
					title:       "Optionally enable ExtendedStatus"
					description: """
						Optionally enable [`ExtendedStatus` option](\(urls.apache_extended_status))
						for more detailed metrics.

						```text file="\(_config_path)"
						# ...

						ExtendedStatus On

						# ...
						```
						"""
					notes: [
						"This defaults to `On` in Apache >= 2.3.6.",
					]
				},
				{
					title: "Apply the Apache config changes"
					description: """
						Start or reload Apache to apply the config changes.
						"""
				},
			]
		}
	}
}
