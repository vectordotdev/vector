package metadata

services: databricks_zerobus: {
	name:     "Databricks Zerobus"
	thing:    "a \(name) ingestion stream"
	url:      urls.databricks
	versions: null

	description: "[Databricks](\(urls.databricks)) is a unified analytics platform. The Zerobus sink streams observability data to Databricks Unity Catalog tables via the Zerobus ingestion service, using protobuf encoding for efficient data transfer."
}
