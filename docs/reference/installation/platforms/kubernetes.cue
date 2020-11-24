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

		kubernetes_api_access: {
			title: "Kubernetes API access control"
			body:  """
				Vector requires access to the Kubernetes API.
				Specifically, the `kubernetes_logs` uses the `/api/v1/pods`
				endpoint to "watch" the pods from all namespaces.

				Modern Kubernetes clusters run with RBAC
				(role-based access control) scheme.
				RBAC-enabled clusters require some configuration to grant Vector
				the authorization to access the Kubernetes API endpoints.
				As RBAC is currently the standard way of controlling access to
				the Kubernetes API, we ship the necessary configuration
				out of the box: see `ClusterRole`, `ClusterRoleBinding` and
				a `ServiceAccount` in our `kubectl` YAML config,
				and the `rbac` configuration at the Helm chart.

				If your cluster doesn't use any access control scheme
				and doesn't restrict access to the Kubernetes API,
				you don't need to do any extra configuration - Vector will
				just work.
				Clusters using legacy ABAC scheme are not officially supported
				(although Vector might work if you configure access properly) -
				we encourage switching to RBAC.
				If you use a custom access control scheme -
				make sure Vector `Pod`/`ServiceAccount` is granted access to
				the `/api/v1/pods` resource.
				"""
		}
	}
}
