package metadata

services: databend: {
	name:     "Databend"
	thing:    "a \(name) database"
	url:      urls.databend
	versions: null

	description: "[Databend](\(urls.databend)) is an open-source Elastic and Workload-Aware Modern Cloud Data Warehouse focusing on Low-Cost and Low-Complexity for your massive-scale analytics needs with the latest techniques in vectorized query processing to allow you to do blazing-fast data analytics on object storage(S3, Azure Blob or MinIO). Open source alternative to Snowflake. Also available in the [Cloud](\(urls.databend_cloud))."
}
