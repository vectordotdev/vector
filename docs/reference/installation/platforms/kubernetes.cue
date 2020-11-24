package metadata

installation: platforms: kubernetes: {
	title:                     "Kubernetes"
	description:               """
		[Kubernetes](\(urls.kubernetes)), also known as k8s, is an
		open-source container-orchestration system for automating
		application deployment, scaling, and management.
		"""
	minimum_supported_version: "1.14"

	how_it_works: components.sources.kubernetes_logs.how_it_works
}
