package metadata

services: gcp_cloud_storage: {
	name:     "GCP Cloud Storage"
	thing:    "a \(name) bucket"
	url:      urls.gcp_cloud_storage
	versions: null

	description: "[Google Cloud Storage](\(urls.gcp_cloud_storage)) is a RESTful online file storage web service for storing and accessing data on Google Cloud Platform infrastructure. The service combines the performance and scalability of Google's cloud with advanced security and sharing capabilities. This makes it a prime candidate for log data."
}
