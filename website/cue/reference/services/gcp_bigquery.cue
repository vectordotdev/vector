package metadata

services: gcp_bigquery: {
	name:     "GCP BigQuery"
	thing:    "a \(name) pipeline"
	url:      urls.gcp_bigquery
	versions: null

	description: "[GCP BigQuery](\(urls.gcp_bigquery)) is a fully-managed data warehouse that allows you to store and query large amounts of structured data. This makes it a great sink for structured log data."
}
