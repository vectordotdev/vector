package metadata

services: mqtt: {
	name:     "MQTT"
	thing:    "\(name) topics"
	url:      urls.mqtt
	versions: null

	description: "[MQTT](\(urls.mqtt)) is an OASIS standard messaging protocol for the Internet of Things (IoT)."
}
