package metadata

services: heroku: {
	name:     "Heroku"
	thing:    "a \(name) app"
	url:      urls.logplex
	versions: null

	description: """
		[Heroku](\(urls.heroku)) is a container-based platform for deploying and
		managing applications. It's a platform as a service (PaaS) that is fully
		managed, allowing developers on Heroku to focus on their applications
		instead of their infrastructure.
		"""

	setup: [
		{
			title: "Setup a Heroku app"
			description: """
				Setup a Heroku app by following the Heroku setup instructions.
				"""
			detour: url: urls.heroku_start
		},
	]

	connect_to: {
		vector: logs: setup: [
			{
				title: "Configure Vector to accept Heroku logs"
				vector: configure: sources: logplex: {
					type:    "logplex"
					address: "0.0.0.0:80"
				}
			},
			{
				title: "Configure TLS termination"
				description: """
					It is _highly_ recommended to configure TLS termination for
					your previously configured Vector logplex address.

					You should either put a load balancer in front of the Vector
					instance to handle TLS termination or configure the `tls` options
					of the Vector `logplex` source to serve a valid certificate.
					"""
				detour: url: urls.aws_elb_https
			},
			{
				title:       "Setup a Heroku log drain"
				description: """
					Using your exposed Vector HTTP address, create a [Heroku log drain](\(urls.heroku_http_log_drain))
					that points to your Vector instance's address:

					```bash
					heroku drains:add https://<user>:<pass>@<address> -a <app>
					```
					"""
			},
		]
	}
}
