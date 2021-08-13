package metadata

services: logstash: {
	name:     "Logstash"
	thing:    name
	url:      urls.logstash
	versions: ">= 0"

	description: "The [Lumberjack protocol](\(urls.logstash_protocol)) is the native protocol used for forwarding messages from the [Logstash](\(urls.logstash) and [Elastic Beats](\(urls.elastic_beats)) agents."
}
