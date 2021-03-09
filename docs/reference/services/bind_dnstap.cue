package metadata

services: bind_dnstap: {
	name:     "BIND dnstap"
	thing:    "the \(name) support for capturing and logging DNS traffic"
	url:      urls.bind_dnstap
	versions: ">= 9.11"

	setup: [
		{
			title:       "Configure BIND to use dnstap"
			description: """
				Enable and configure BIND to use dnstap by following ISC KB article 
				[Using DNSTAP with BIND](\(urls.bind_dnstap)).
				"""
			detour: url: urls.bind_dnstap
		},
		{
			title: "Configure Vector to accept BIND dnstap output"
			vector: configure: sources: dnstap: {
				type:             "dnstap"
				socket_path:      "/run/bind/dnstap.sock"
				socket_file_mode: 508
			}
		},
	]
}
