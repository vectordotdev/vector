package metadata

services: gcp_bigquery: {
	name:     "GCP BigQuery"
	thing:    "a \(name) table"
	url:      urls.gcp_bigquery
	versions: null

	description: "[Google BigQuery](\(urls.gcp_bigquery)) is a serverless data warehouse."
}
