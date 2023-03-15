package metadata

services: envoy_als: {
	name:     "Envoy ALS"
	thing:    "an \(name) server"
	url:      urls.envoy_als
	versions: "v1.24.1"
	description: "[Envoy](\(urls.envoy)) is a high performance load balancer designed for modern service oriented architectures."
}
