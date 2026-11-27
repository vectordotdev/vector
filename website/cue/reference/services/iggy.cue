package metadata

services: iggy: {
	name:     "Iggy"
	thing:    "an \(name) message broker"
	url:      urls.iggy
	versions: null

	description: "[Iggy](\(urls.iggy)) is a persistent message streaming platform written in Rust, supporting QUIC, TCP, HTTP, and WebSocket transport protocols, capable of processing millions of messages per second."
}
