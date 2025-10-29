package metadata

services: azure_logs_ingestion: {
	name:     "Azure Logs Ingestion"
	thing:    "a \(name) account"
	url:      urls.azure_monitor
	versions: null

	description: "[Azure Monitor](\(urls.azure_monitor)) is a service in Azure that provides performance and availability monitoring for applications and services in Azure, other cloud environments, or on-premises. Azure Monitor collects data from multiple sources into a common data platform (Log Analytics) where it can be analyzed for trends and anomalies."
}
