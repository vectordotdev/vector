package metadata

services: mongodb: {
	name:     "MongoDB"
	thing:    "an \(name) instance"
	url:      urls.mongodb
	versions: null

	description: "[MongoDB](\(urls.mongodb)) is a general purpose, document-based, distributed database built for modern application developers and for the cloud era."
}
