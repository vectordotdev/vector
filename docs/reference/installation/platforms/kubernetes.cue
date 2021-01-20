package metadata

installation: platforms: kubernetes: {
	title:                     "Kubernetes"
	description:               """
		[Kubernetes](\(urls.kubernetes)), also known as k8s, is an
		open-source container-orchestration system for automating
		application deployment, scaling, and management.
		"""
	minimum_supported_version: "1.14"

	how_it_works: {
		components.sources.kubernetes_logs.how_it_works

		metrics: {
			title: "Metrics"
			body: """
				Our Helm chart deployments provide quality of life around setup and maintenance of
				metrics pipelines in Kubernetes. Each of the Helm charts provide an `internal_metrics`
				source and `prometheus` sink out of the box. Agent deployments also expose `host_metrics`
				via the same `prometheus` sink.

				Charts come with options to enable Prometheus integration via annotations or Prometheus Operator
				integration via PodMonitor. Thus, the Prometheus node_exporter agent is not required when the `host_metrics` source is
				enabled.
				"""
		}
	}
}
