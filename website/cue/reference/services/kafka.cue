package metadata

services: kafka: {
	name:     "Kafka"
	thing:    "\(name) topics"
	url:      urls.kafka
	versions: ">= 2.4"

	description: "[Apache Kafka](\(urls.kafka)) is an open-source project for a distributed publish-subscribe messaging system rethought as a distributed commit log. Kafka stores messages in topics that are partitioned and replicated across multiple brokers in a cluster. Producers send messages to topics from which consumers read. These features make it an excellent candidate for durably storing logs and metrics data."
}
