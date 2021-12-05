package metadata

services: gcp_cloud_monitoring: {
	name:     "GCP Cloud (formerly Stackdriver) metrics"
	thing:    "a \(name) account"
	url:      urls.gcp_stackdriver_metrics
	versions: null

	description: "[Stackdriver](\(urls.gcp_stackdriver)) is Google Cloud's embedded observability suite designed to monitor, troubleshoot, and improve cloud infrastructure, software and application performance. Stackdriver enables you to efficiently build and run workloads, keeping applications available and performing well."
}
