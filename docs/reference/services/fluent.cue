package metadata

services: fluent: {
	name:     "Fluent"
	thing:    name
	url:      urls.fluent
	versions: ">= 0"

	description: "The [Fluent protocol](\(urls.fluent)) is the native protocol used for forwarding messages from the [fluentd](\(urls.fluentd) and [fluent-bit](\(urls.fluentbit)) agents."
}
