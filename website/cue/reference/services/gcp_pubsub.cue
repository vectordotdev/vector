package metadata

services: gcp_pubsub: {
	name:     "GCP PubSub"
	thing:    "a \(name) pipeline"
	url:      urls.gcp_pubsub
	versions: null

	description: "[GCP Pub/Sub](\(urls.gcp_pubsub)) is a fully-managed real-time messaging service that allows you to send and receive messages between independent applications on the Google Cloud Platform."
}
