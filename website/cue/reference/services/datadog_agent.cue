package metadata

services: datadog_agent: {
	name:     "Datadog Agent"
	thing:    "a \(name)"
	url:      urls.datadog_agent
	versions: null

	description: "The [Datadog agent](\(urls.datadog_agent)) is a daemon that collects eventst and metrics from hosts to forward to Datadog, but can also be sent to Vector."
}
