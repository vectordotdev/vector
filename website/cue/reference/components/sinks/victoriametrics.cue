package metadata

components: sinks: victoriametrics: {
	title: "VictoriaMetrics"

	classes: {
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: []
		stateful: true
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		healthcheck: {
			enabled:  true
			uses_uri: true
		}
		send: {
			batch: {
				enabled:      true
				max_bytes:    8_388_608
				max_events:   10_000
				timeout_secs: 1.0
			}
			// zstd is always used; only the level is configurable with `compression_level`.
			compression: enabled: false
			encoding: enabled:    false
			proxy: enabled:       true
			request: {
				enabled:                    true
				rate_limit_duration_secs:   1
				rate_limit_num:             9_223_372_036_854_775_807
				retry_initial_backoff_secs: 1
				retry_max_duration_secs:    30
				timeout_secs:               60
				headers:                    true
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      true
			}
			to: {
				service: services.victoriametrics

				interface: {
					socket: {
						api: {
							title: "VictoriaMetrics remote write protocol"
							url:   urls.victoriametrics_remote_write
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "optional"
					}
				}
			}
		}
	}

	support: {
		requirements: [
			"""
				VictoriaMetrics must support the VictoriaMetrics remote write protocol (zstd-compressed
				remote write). The Prometheus remote write protocol (snappy) is not used.
				""",
		]
		warnings: []
	}

	configuration: generated.components.sinks.victoriametrics.configuration

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
		traces: false
	}

	how_it_works: {
		protocol: {
			title: "Protocol"
			body:  """
				Metrics are sent as a Prometheus remote write `WriteRequest` protobuf compressed with
				zstd, with the `X-VictoriaMetrics-Remote-Write-Version: 1` header. This is the
				[protocol](\(urls.victoriametrics_remote_write)) `vmagent` uses to send data to
				VictoriaMetrics, and needs less bandwidth than the snappy-compressed Prometheus
				protocol.

				As in `vmagent`, each request is compressed in a single zstd frame that records its
				size, and `batch.max_bytes` bounds the estimated uncompressed size of a request
				(8 MiB by default, like `-remoteWrite.maxBlockSize`). This keeps requests below the
				`-maxInsertRequestSize` limit of VictoriaMetrics even for distributions, which expand
				to many series.
				"""
		}
		distributions: {
			title: "Distributions"
			body:  """
				Distributions are sent as VictoriaMetrics
				[`vmrange` histograms](\(urls.victoriametrics_histograms)): `<name>_bucket` series with
				a `vmrange` label for each non-empty bucket, plus `<name>_sum` and `<name>_count`.
				Buckets are log-scale with 18 buckets per power of ten, so no bucket boundaries need
				to be configured. The layout matches the VictoriaMetrics client libraries, so
				`histogram_quantile` aggregates series from Vector and from instrumented applications
				together.

				Incremental distributions are accumulated per series in bounded memory. Use
				`expire_metrics_secs` to drop series that stop receiving data.
				"""
		}
		tenants: {
			title: "Tenants"
			body:  """
				The `tenant.mode` option chooses how the
				[tenant](\(urls.victoriametrics_cluster_tenancy)) of a VictoriaMetrics cluster is sent:

				* `none`: no tenant. Use this for single-node VictoriaMetrics and for `vmauth`.
				* `path`: the tenant is part of the `vminsert` URL. Batches are split per tenant.
				* `labels`: the tenant is sent in the `vm_account_id` and `vm_project_id` labels to the
				  `vminsert` multitenant endpoint.

				`tenant.id` is a template rendering `accountID` or `accountID:projectID`. Events whose
				rendered tenant is not valid are rejected. Templates with dynamic content need a
				literal prefix, such as `42:{{ tags.project_id }}`, unless template confinement is
				disabled.
				"""
		}
		vmauth: {
			title: "Per-event credentials with vmauth"
			body:  """
				[`vmauth`](\(urls.victoriametrics_vmauth)) chooses the tenant from the credential of a
				request. To route events to different tenants, set the `victoriametrics_token` event
				secret in a `remap` transform, for example
				`set_secret("victoriametrics_token", get_env_var!("VM_TOKEN_" + upcase!(.tags.team)))`.
				The secret is sent as a bearer token instead of the configured `auth`, and batches
				are split per token.
				"""
		}
		retries: {
			title: "Retries"
			body: """
				With the default `retry_strategy`, the sink behaves like `vmagent`: requests rejected
				with 400 (invalid data), 409, or 415 are dropped, and every other failed request is
				retried. This includes 401 and 403, which `vmauth` returns while credentials are
				rotated.
				"""
		}
		healthcheck: {
			title: "Healthcheck"
			body: """
				The healthcheck writes an empty request to the write path with the configured
				credentials, which checks reachability, authentication, and `vmauth` routing. It is
				skipped when the write path depends on event data.
				"""
		}
		sketches: {
			title: "Sketches"
			body: """
				Sketches are sent as summaries with the 0.5, 0.75, 0.9, 0.95, and 0.99 quantiles, plus
				`_sum`, `_count`, `_min`, and `_max` series, like VictoriaMetrics stores sketches it
				receives from the Datadog agent.
				"""
		}
		tenant_labels: {
			title: "Tenant labels"
			body: """
				In `labels` mode, both `vm_account_id` and `vm_project_id` are written on every series
				(the project defaults to `0`), and metric metadata carries the tenant in the fields
				`vminsert` reads for the multitenant endpoint.
				"""
		}
		duplicate_tag_names: {
			title: "Duplicate tag names"
			body: """
				When a tag has multiple values, only the last value is sent.
				"""
		}
	}
}
