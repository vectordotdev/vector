package metadata

services: nats: {
	name:     "NATS"
	thing:    "a \(name) server"
	url:      urls.nats
	versions: null

	description: "[NATS.io](\(urls.nats)) is a simple, secure and high performance open source messaging system for cloud native applications, IoT messaging, and microservices architectures. NATS.io is a Cloud Native Computing Foundation project."
}
