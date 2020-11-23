package metadata

installation: platforms: kubernetes: {
	title:                     "Kubernetes"
	description:               """
		[Kubernetes](\(urls.kubernetes)), also known as k8s, is an
		open-source container-orchestration system for automating
		application deployment, scaling, and management.
		"""
	minimum_supported_version: "1.14"

	how_it_works: components.sources.kubernetes_logs.how_it_works & {
		testing_and_reliability: {
			title: "Testing & reliability"
			body:  """
					Vector is tested extensively against Kubernetes. In addition to Kubernetes
					being Vector's most popular installation method, Vector implements a
					comprehensive end-to-end test suite for all minor Kubernetes versions starting
					with `\(minimum_supported_version)`.
					"""
		}
	}
}
