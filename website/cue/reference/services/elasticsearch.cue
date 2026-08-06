package metadata

services: elasticsearch: {
	name:     "Elasticsearch"
	thing:    "an \(name) database"
	url:      urls.elasticsearch
	versions: null

	description: "[Elasticsearch](\(urls.elasticsearch)) is a search engine based on the Lucene library. It provides a distributed, multitenant-capable full-text search engine with an HTTP web interface and schema-free JSON documents. As a result, it is very commonly used to store and analyze log data. It ships with Kibana which is a simple interface for visualizing and exploring data in Elasticsearch."
}
