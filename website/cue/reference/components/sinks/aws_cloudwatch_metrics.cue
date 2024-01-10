package metadata

components: sinks: aws_cloudwatch_metrics: components._aws & {
	title: "AWS Cloudwatch Metrics"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["AWS"]
		stateful: false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_events:   20
				timeout_secs: 1.0
			}
			compression: {
				enabled: true
				default: "none"
				algorithms: ["none", "gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: enabled: false
			proxy: enabled:    true
			request: enabled:  false
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
			}
			to: {
				service: services.aws_cloudwatch_metrics

				interface: {
					socket: {
						api: {
							title: "AWS Cloudwatch metrics API"
							url:   urls.aws_cloudwatch_metrics_api
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "required"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		warnings: [
			"""
				Gauge values are persisted between flushes. On Vector start up each
				gauge is assumed to have zero, 0.0, value, that can be updated
				explicitly by the consequent absolute, not delta, gauge observation,
				or by delta increments/decrements. Delta gauges are considered an
				advanced feature useful in a distributed setting, however they
				should be used with care.
				""",
		]
		notices: [
			"""
				CloudWatch Metrics types are organized not by their semantics, but
				by storage properties:

				* Statistic Sets
				* Data Points

				In Vector only the latter is used to allow lossless statistics
				calculations on CloudWatch side.
				""",
		]
	}

	configuration: base.components.sinks.aws_cloudwatch_metrics.configuration & {
		_aws_include: false
	}

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    false
			set:          false
			summary:      false
		}
		traces: false
	}

	permissions: iam: [
		{
			platform:  "aws"
			_service:  "cloudwatch"
			_docs_tag: "AmazonCloudWatch"

			policies: [
				{
					_action: "PutMetricData"
					required_for: ["healthcheck", "operation"]
				},
			]
		},
	]
}
