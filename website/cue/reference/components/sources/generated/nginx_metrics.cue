package metadata

generated: components: sources: nginx_metrics: configuration: {
	auth: {
		description: """
			Configuration of the authentication strategy for HTTP requests.

			HTTP authentication should be used with HTTPS only, as the authentication credentials are passed as an
			HTTP header without any additional encryption beyond what is provided by the transport itself.
			"""
		required: false
		type:     _schemaDefinitions["core::option::Option<vector::http::Auth>"]
	}
	endpoints: {
		description: """
			A list of NGINX instances to scrape.

			Each endpoint must be a valid HTTP/HTTPS URI pointing to an NGINX instance that has the
			`ngx_http_stub_status_module` module enabled.
			"""
		required: true
		type: array: items: type: string: examples: ["http://localhost:8000/basic_status"]
	}
	namespace: {
		description: """
			Overrides the default namespace for the metrics emitted by the source.

			If set to an empty string, no namespace is added to the metrics.

			By default, `nginx` is used.
			"""
		required: false
		type: string: default: "nginx"
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
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
