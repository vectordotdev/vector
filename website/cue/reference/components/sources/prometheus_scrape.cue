package metadata

components: sources: prometheus_scrape: {
	title: "Prometheus Scrape"

	classes: {
		delivery: "at_least_once"
		deployment_roles: ["daemon", "sidecar"]
		development:   "stable"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		auto_generated:   true
		acknowledgements: false
		collect: {
			checkpoint: enabled: false
			from: {
				service: services.prometheus_client

				interface: socket: {
					api: {
						title: "Prometheus"
						url:   urls.prometheus_text_based_exposition_format
					}
					direction: "outgoing"
					protocols: ["http"]
					ssl: "optional"
				}
			}
			proxy: enabled: true
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      true
			}
		}
		multiline: enabled: false
	}

	support: {
		requirements: []
		warnings: []
	}

	installation: {
		platform_name: null
	}

	configuration: generated.components.sources.prometheus_scrape.configuration & {
		endpoints: warnings: ["You must explicitly add the path to your endpoints. Vector will _not_ automatically add `/metrics`."]
	}

	how_it_works: {
		duplicate_tag_names: {
			title: "Duplicate tag names"
			body: """
				Multiple tags with the same name are invalid within Prometheus. Prometheus
				itself will reject a metric with duplicate tags. Vector will accept the metric,
				but will only take the last value for each tag name specified.
				"""
		}

		query_params_structure: {
			title: "Query params structure"
			body: """
				In query params, key needs to be `match[]` with array of values

				```yaml
				sources:
					source0:
						query:
							"match[]":
								- '{job="somejob"}'
								- '{__name__=~"job:.*"}'
				```
				"""
		}

		kubernetes_pod_discovery: {
			title: "Kubernetes Pod discovery"
			body: """
				A `kubernetes` entry in `targets` discovers scrape targets from Kubernetes Pods. Vector
				watches Pods through the Kubernetes API and scrapes every Pod annotated with
				`prometheus.io/scrape: "true"`. These annotations control each target:

				| Annotation | Default | Description |
				| --- | --- | --- |
				| `prometheus.io/scrape` | | Must be `"true"` for the Pod to be scraped. |
				| `prometheus.io/port` | See below | The port number or the name of a container port. |
				| `prometheus.io/path` | `default_path` | The path to scrape. |
				| `prometheus.io/scheme` | `default_scheme` | `http` or `https`. |
				| `prometheus.io/param_<name>` | | Adds `<name>` as a query parameter. |

				`annotation_prefix` changes the `prometheus.io` prefix, and `namespaces` and
				`label_selector` limit which Pods are watched.

				Without a port annotation, Vector scrapes the TCP container ports named `metrics`,
				`prometheus`, or `http-metrics`, or else the first TCP container port. Only regular
				containers (`spec.containers`) are considered, not init or ephemeral containers. Vector
				uses the Pod's primary IP address (`status.podIP`).

				Vector skips Pods that have no IP address yet and Pods in the `Pending`, `Succeeded`, or
				`Failed` phase. Pods in the `Unknown` phase are still scraped, and readiness isn't
				required.

				Metrics from discovered Pods get the `namespace`, `pod`, `node`, and `container` tags,
				plus the labels and annotations listed in `pod_label_tags` and `pod_annotation_tags`.
				Static URLs and discovered Pods can be scraped by the same source:

				```yaml
				sources:
				  prometheus:
				    type: prometheus_scrape
				    targets:
				      - static:
				          urls:
				            - http://localhost:9090/metrics
				      - kubernetes:
				          role: pod
				          namespaces: [monitoring]
				          label_selector: "app.kubernetes.io/part-of=my-app"
				```

				Only `role: pod` is supported, and a source can have one `kubernetes` entry.
				"""
		}

		kubernetes_permissions: {
			title: "Kubernetes permissions"
			body: """
				Vector's service account needs permission to `list` and `watch` Pods. Without
				`namespaces`, Vector watches every namespace, which needs a `ClusterRole`:

				```yaml
				apiVersion: rbac.authorization.k8s.io/v1
				kind: ClusterRole
				metadata:
				  name: vector-prometheus-pod-discovery
				rules:
				  - apiGroups: [""]
				    resources: ["pods"]
				    verbs: ["list", "watch"]
				---
				apiVersion: rbac.authorization.k8s.io/v1
				kind: ClusterRoleBinding
				metadata:
				  name: vector-prometheus-pod-discovery
				roleRef:
				  apiGroup: rbac.authorization.k8s.io
				  kind: ClusterRole
				  name: vector-prometheus-pod-discovery
				subjects:
				  - kind: ServiceAccount
				    name: vector
				    namespace: vector
				```

				With `namespaces`, a `Role` and `RoleBinding` in each listed namespace is enough:

				```yaml
				apiVersion: rbac.authorization.k8s.io/v1
				kind: Role
				metadata:
				  name: vector-prometheus-pod-discovery
				  namespace: monitoring
				rules:
				  - apiGroups: [""]
				    resources: ["pods"]
				    verbs: ["list", "watch"]
				---
				apiVersion: rbac.authorization.k8s.io/v1
				kind: RoleBinding
				metadata:
				  name: vector-prometheus-pod-discovery
				  namespace: monitoring
				roleRef:
				  apiGroup: rbac.authorization.k8s.io
				  kind: Role
				  name: vector-prometheus-pod-discovery
				subjects:
				  - kind: ServiceAccount
				    name: vector
				    namespace: vector
				```
				"""
		}

		kubernetes_local_node: {
			title: "Scraping only the local node"
			body: """
				When Vector runs as a DaemonSet, set `use_self_node_only: true` so that each Vector
				instance only scrapes the Pods on its own node. Vector reads the node name from the
				`VECTOR_SELF_NODE_NAME` environment variable, or from `self_node_name`. Set the
				variable from the Pod spec:

				```yaml
				env:
				  - name: VECTOR_SELF_NODE_NAME
				    valueFrom:
				      fieldRef:
				        fieldPath: spec.nodeName
				```
				"""
		}
	}

	output: metrics: {
		_extra_tags: {
			"instance": {
				description: "The host:port of the scraped instance. Only present if `instance_tag` is set."
				examples: ["localhost:9090"]
				required: false
			}
			"endpoint": {
				description: "Any endpoint of the scraped instance. Only present if `endpoint_tag` is set."
				examples: ["http://localhost:9090/metrics"]
				required: false
			}
			"namespace": {
				description: "The namespace of the scraped Pod. Only present for targets discovered from Kubernetes."
				examples: ["monitoring"]
				required: false
			}
			"pod": {
				description: "The name of the scraped Pod. Only present for targets discovered from Kubernetes."
				examples: ["node-exporter-7d9c4"]
				required: false
			}
			"node": {
				description: "The node the scraped Pod runs on. Only present for targets discovered from Kubernetes."
				examples: ["node-01"]
				required: false
			}
			"container": {
				description: "The container that declares the scraped port. Only present for targets discovered from Kubernetes."
				examples: ["exporter"]
				required: false
			}
		}

		counter: output._passthrough_counter & {
			tags: _extra_tags
		}
		gauge: output._passthrough_gauge & {
			tags: _extra_tags
		}
		histogram: output._passthrough_histogram & {
			tags: _extra_tags
		}
		summary: output._passthrough_summary & {
			tags: _extra_tags
		}
	}

	telemetry: metrics: {
		http_client_responses_total:                 components.sources.internal_metrics.output.metrics.http_client_responses_total
		http_client_response_rtt_seconds:            components.sources.internal_metrics.output.metrics.http_client_response_rtt_seconds
		prometheus_kubernetes_sd_targets_discovered: components.sources.internal_metrics.output.metrics.prometheus_kubernetes_sd_targets_discovered
	}
}
