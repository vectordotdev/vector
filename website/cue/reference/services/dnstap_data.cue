package metadata

services: dnstap_data: {
	name:     "dnstap data produced by DNS server"
	thing:    "the \(name) conveys requests/responses of DNS queries/updates"
	url:      urls.dnstap
	versions: null

	setup: [
		{
			title: "Configure DNS server to produce dnstap data"
			description: """
				Taking ISC DNS server BIND as an example, to enable and configure
				BIND to produce dnstap data, follow instructions in ISC KB article
				[Using DNSTAP with BIND]((urls.bind_dnstap)).
				"""
			notes: [
				"""
					The DNS server needs to be configured to write dnstap data into the
					unix domain socket to be created by Vector (described below).
					""",
			]
		},
		{
			title: "Configure Vector dnstap source component to receive dnstap data output"
			description: """
				Configure Vector dnstap source component to create a server unix
				domain socket for DNS server to write dnstap data.
				"""

			notes: [
				"""
					The unix domain socket created should be writable for the DNS server process.
					""",
			]
			vector: configure: sources: dnstap: {
				type:             "dnstap"
				socket_path:      "/run/bind/dnstap.sock"
				socket_file_mode: 508
			}
		},
	]
}
