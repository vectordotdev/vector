package metadata

releases: "0.21.0": {
	date:     "2022-04-11"
	codename: ""

	known_issues: [
		"The `kubernetes_logs` source can panic when while processing Kubernetes watcher events when there is an error. [#12245](https://github.com/vectordotdev/vector/issues/12245). Fixed in `0.21.1`.",
		"The `elasticsearch` sink fails to include the security token when signing requests for AWS authentication to OpenSearch. [#12249](https://github.com/vectordotdev/vector/issues/12249). Fixed in `0.21.1`.",
		"The `nats` source and sink authentication options were not configurable. [#12262](https://github.com/vectordotdev/vector/issues/12262). Fixed in `0.21.1`.",
		"The `internal_logs` source includes excess trace logs whenever `vector top` is used. [#12251](https://github.com/vectordotdev/vector/issues/12251). Fixed in `0.21.1`.",
		"The `aws_cloudwatch_logs` source does not handle throttle responses from AWS. [#12253](https://github.com/vectordotdev/vector/issues/12253). Fixed in `0.21.1`.",
		"Vector panics when loading configuration that includes event paths like `encoding.only_fields`. [#12256](https://github.com/vectordotdev/vector/issues/12256). Fixed in `0.21.1`.",
		"Vector panicked when reloading configuration that added components to a running topology. [#12273](https://github.com/vectordotdev/vector/issues/12273). Fixed in `0.21.1`.",
		"Using `assume_role` on AWS components did not function correctly. [#12314](https://github.com/vectordotdev/vector/issues/12314). Fixed in `0.21.1`.",
		"The Vector VRL REPL loses variable assignments if the expression being evaluated errors. [#12400](https://github.com/vectordotdev/vector/issues/12400). Fixed in `0.21.2`.",
		"Vector docker images require a volume to be mounted at `/var/lib/vector` to start correctly when the default `data_dir` of is used. [#12413](https://github.com/vectordotdev/vector/issues/12413). Fixed in `0.21.2`.",
		"For AWS components, the timeout for loading credentials was dropped from 30 seconds to 5 seconds. [#12421](https://github.com/vectordotdev/vector/issues/12421). `0.21.2` adds a new option, `load_timeout_secs` that can be configured to a higher value.",
		"`vector generate` works again with the `datadog_agent` source. [#12469](https://github.com/vectordotdev/vector/issues/12469). Fixed in `0.21.2`.",
		"Using `assume_role` configuration for AWS components doesn't cache the credentials, resulting in a high number of calls to `AssumeRole`. This was fixed in `0.22.0` via [awslabs/smithy-rs#1296](https://github.com/awslabs/smithy-rs/pull/1296).",
	]

	whats_next: [
		{
			title: "An OpenTelemetry source"
			description: """
				We are [in the process](https://github.com/vectordotdev/vector/pull/11802) of adding a source for
				ingesting data from the OpenTelemetry collector and OpenTelemetry compatible tools. We are starting with
				traces, since this has stabilized, but will move on to metrics and logs.

				We'll also be adding an OpenTelemetry sink for forwarding data from Vector to OpenTelemetry-compatible APIs.
				"""
		},
		{
			title:       "Component metric standardization"
			description: """
				We continue to be in the process of ensuring that all Vector components report a consistent set of
				metrics to make it easier to monitor the performance of Vector.  These metrics are outlined in this new
				[instrumentation specification](\(urls.specs_instrumentation).
				"""
		},
		{
			title: "Official release of end-to-end acknowledgements feature"
			description: """
				We have started to add support for end-to-end acknowledgements from sources to sinks where sources will
				not ack data until the data has been processed by all associated sinks. It is usable by most components
				now, but we expect to officially release this feature after some final revisions, testing, and documentation.
				"""
		},
		{
			title: "VRL iteration support"
			description: """
				At long last, support for iteration in VRL is almost ready. We expect it to be included in the next
				release.

				See the
				[RFC](https://github.com/vectordotdev/vector/blob/master/rfcs/2021-08-29-8381-vrl-iteration-support.md)
				for a preview of how this will work.
				"""
		},
		{
			type: "fix"
			scopes: ["vrl"]
			breaking: true
			description: """
				VRL now has lexical scoping for blocks. This means that variables defined inside of a block in VRL (e.g.
				an `if` condition block) are no longer accessible from outside of this block. This breaking change
				was done to support VRL's forthcoming iteration feature which requires it.

				See the [upgrade guide](/highlights/2022-03-22-0-21-0-upgrade-guide#vrl-lexical-scoping) for how to
				migrate your VRL programs.
				"""
			pr_numbers: [12017]
		},
	]

	description: """
		The Vector team is pleased to announce version 0.21.0!

		Be sure to check out the [upgrade guide](/highlights/2022-03-22-0-21-0-upgrade-guide) for breaking changes in
		this release.

		In addition to the new features, enhancements, and fixes listed below, this release adds:

		- A new implementation of the VRL runtime as a Virtual Machine (VM). This new implementation improves
		  performance over VRL's current tree-walking interpreter implementation. For its initial release, this is an opt in
		  feature (see [the highlight for how](/highlights/2022-03-15-vrl-vm-beta)) but will become the default VRL
		  implementation in the future once it has stabilized. We encourage you to try it out and
		  [report](https://github.com/vectordotdev/vector/issues/new?assignees=&labels=type%3A+bug&template=bug.yml) any
		  issues you find.
		- A new `redis` source to complement the `redis` sink.
		- Initial support for ingesting traces from the Datadog Agent (version < 6/7.33) and forwarding them to Datadog.
		  We are working on adding support for newer Datadog Agents.
		- The `kubernetes_logs` source has been rewritten to use the community supported
		  [`kube-rs`](https://github.com/kube-rs/kube-rs) library. We expect that this will resolve some long
		  outstanding bugs with Vector ceasing to process container logs. It also adds support for Kubernetes
		  authentication token rotation.

		We made additional performance improvements this release increasing the average throughput by up to 50% for
		common topologies (see our [soak test
		framework](https://github.com/vectordotdev/vector/tree/master/soaks/tests)).

		Also, check out our [new guide](/guides/level-up/vector-tap-guide) on using `vector tap` for observing events
		running through Vector instances
		"""

	changelog: [
		{
			type: "feat"
			scopes: ["vrl", "performance"]
			description: """
				This release includes a beta of a new implementation of VRL as a Virtual Machine. This new
				implementation improves on VRL performance and otherwise exactly compatible with the existing VRL
				implementation. This is an opt-in feature for this release to gather feedback (see [the highlight for
				how to enable](/highlights/2022-03-15-vrl-vm-beta)) but will become the default VRL
				implementation in the future once it stabilizes.
				"""
			pr_numbers: [11554]
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				We are in the process of updating all Vector components with consistent instrumentation as described in
				[Vector's component
				specification](\(urls.specs_instrumentation)).

				With this release we have instrumented the following sources with these new metrics:

				- `mongodb_metrics`
				- `postgresql_metrics`
				- `socket`
				- `statsd`

				As well as all transforms.
				"""
			pr_numbers: [11104, 11223, 11122, 11124, 11125]
		},
		{
			type: "fix"
			scopes: ["api", "observability"]
			description: """
				Vector's `/health` endpoint (mounted when `api.enabled` is `true`) now returns a 503 when Vector is
				shutting down. This is useful when using a load balancer so that traffic is routed to other running
				Vector instances.
				"""
			pr_numbers: [11183]
		},
		{
			type: "chore"
			scopes: ["api"]
			breaking: true
			description: """
				Deprecated GraphQL API routes were removed. See the [upgrade
				guide](/highlights/2022-03-22-0-21-0-upgrade-guide) for more details
				"""
			pr_numbers: [11364]
		},
		{
			type: "enhancement"
			scopes: ["aws provider", "sinks"]
			description: """
				The `tls` options can now be configured on AWS sinks. This is useful when using AWS compatible endpoints
				where the certificates may not be trusted by the local store.
				"""
			pr_numbers: [10314]
		},
		{
			type: "enhancement"
			scopes: ["delivery", "sinks"]
			description: """
				The end-to-end acknowledgements configuration, `acknowledgements`, was moved from sources to sinks. When
				set on a sink, all connected sources that support acknowledgements are configured to wait for the sink
				to acknowledge before acknowledging the client. Setting `acknowledgements` on sources is now deprecated.

				See the [upgrade guide](/highlights/2022-03-22-0-21-0-upgrade-guide#sink-acks) for more details.
				"""
			pr_numbers: [11346]
		},
		{
			type: "fix"
			scopes: ["lua transform", "config"]
			description: """
				The `lua` transform now returns an error at configuration load time if an unknown field is present on
				`hooks`. This helps make typos more visible.
				"""
			pr_numbers: [11459]
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				`vector tap` and `vector top` have had a few enhancements.

				`vector tap`:

				* It now supports tapping inputs via `--inputs-of`
				* It now reports when the component id patterns provided do not match. It supports `--quiet` to suppress
				  these messages.
				* A `--meta` flag was added to include metadata about which component the output events came from.

				Both `vector top` and `vector tap` now automatically reconnect if the remote Vector instance goes away.
				This behavior can be disabled by passing `--no-reconnect`.
				"""
			pr_numbers: [11293, 11321, 11531, 11589]
		},
		{
			type: "fix"
			scopes: ["docker_logs source"]
			description: """
				`docker_logs` source now exits, shutting down Vector, if it hits
				an unrecoverable deserialization error. Previously it would just
				stall.
				"""
			pr_numbers: [11487]
		},
		{
			type: "fix"
			scopes: ["aws provider"]
			description: """
				AWS components now allow `region` and `endpoint` to be
				configured simultaneously. This is useful when using an AWS
				compatible API.
				"""
			pr_numbers: [11578]
		},
		{
			type: "enhancement"
			scopes: ["datadog_agent source", "datadog_trace sink"]
			description: """
				Initial support was added for ingesting traces from the Datadog Agent into Vector (via the
				`datadog_agent` source) and forwarding them to the Datadog API (via the new `datadog_traces` sink). Note
				that currently APM metrics are dropped and so you will be missing these statistics in Datadog if you
				forward traces to it through Vector. We will be following up to add support APM metrics to Vector.

				Datadog docs are forthcoming but the Agent configuration option, `apm_config.apm_dd_url`, can be used to
				forward traces from the Datadog Agent to Vector.
				"""
			pr_numbers: [11033, 11489]
		},
		{
			type: "fix"
			scopes: ["gcp_stackdriver_logs sink"]
			description: """
				The `gcp_stackdriver_logs` sink now recognizes a severity of `ER` as `ERROR`.
				"""
			pr_numbers: [11658]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			breaking: true
			description: """
				The remainder operation in VRL is now fallible if the right-hand side is a field or variable as this can
				fail at runtime if the right-hand side is `0`. This matches the behavior of division.
				"""
			pr_numbers: [11668]
		},
		{
			type: "fix"
			scopes: ["reload"]
			description: """
				A case where Vector would panic during reload was fixed that would occur whenever a component has
				changing inputs, but some inputs are the same.
				"""
			pr_numbers: [11680]
		},
		{
			type: "enhancement"
			scopes: ["internal_metrics source"]
			description: """
				The `scrape_interval_secs` configuration option of the `internal_metrics` source can now be fractional
				seconds.
				"""
			pr_numbers: [11673]
		},
		{
			type: "fix"
			scopes: ["journald source"]
			description: """
				The `journald` source now flushes internal batches every 10 milliseconds, regardless of whether the
				batch is full. This avoids an issue where the source would wait a very long time to send data downstream
				when the volume was low but the batch size was configured high to handle spikes.
				"""
			pr_numbers: [11671]
		},
		{
			type: "enhancement"
			scopes: ["vrl"]
			description: """
				VRL's `to_timestamp` now accepts an optional `unit` argument to control how numeric unix timestamp
				arguments are interpreted. For example, `unit` can be set to `milliseconds` if the incoming timestamps
				are unix millisecond timestamps. It defaults to `seconds` to maintain current behavior.
				"""
			pr_numbers: [11663]
		},
		{
			type: "enhancement"
			scopes: ["releasing"]
			description: """
				For Debian packages, the created `vector` user is now added to the `systemd-journal-remote` group, if it
				exists, to facilitate Vector being used to collect remote journald logs.
				"""
			pr_numbers: [11713]
		},
		{
			type: "feat"
			scopes: ["nats sink", "nats source"]
			description: """
				The `nats` sink and `nats` source now support TLS and authentication via username/password, JWT, Token,
				NKey, and client certificate.
				"""
			pr_numbers: [10688]
		},
		{
			type: "feat"
			scopes: ["sources"]
			description: """
				A new `redis` source was added to complement the existing `redis` sink. It supports fetching data via
				subscribing to a pub/sub channel or popping from a list.
				"""
			pr_numbers: [7096]
		},
		{
			type: "enhancement"
			scopes: ["loki sink"]
			description: """
				The `loki` sink now supports setting `out_of_order_action` to `accept` to instruct Vector to not modify
				event timestamps. Vector would previously modify timestamps to attempt to satisfy Loki's ordering
				constraints, but these constraints were relaxed in Loki 2.4. If you are running Loki >= 2.4 it is
				recommended to set `out_of_order_action` to `accept` to enable Vector to send data concurrently.
				"""
			pr_numbers: [11133, 11761]
		},
		{
			type: "enhancement"
			scopes: ["aws provider"]
			description: """
				All AWS components were migrated to the new [AWS SDK](https://github.com/awslabs/aws-sdk-rust) from the
				end-of-life rusoto SDK. This new SDK supports IMSDv2 for authentication.

				See the [upgrade guide](/highlights/2022-03-22-0-21-0-upgrade-guide#aws-sdk-migration) for more
				information.
				"""
			pr_numbers: [11752, 11777, 11781, 11868, 11881, 11906, 11939, 11853]
		},
		{
			type: "enhancement"
			scopes: ["journald source"]
			description: """
				The `journald` source now supports a `since_now` option to instruct Vector to only fetch journal entries
				that occur after Vector starts.
				"""
			pr_numbers: [11799]
		},
		{
			type: "fix"
			scopes: ["geoip transform"]
			description: """
				The `geoip` transform now avoids re-reading the database from disk randomly. This was unintended
				behavior. We have an [open issue](https://github.com/vectordotdev/vector/issues/11817) for reloading the
				database from disk during Vector's reload process.
				"""
			pr_numbers: [11831]
		},
		{
			type: "fix"
			scopes: ["observability"]
			description: """
				Ensure that instrumentation labels on internal logs and metrics are not lost when Vector is run in quiet
				mode (`-q` or `-qq`). Previously running with a log level below `INFO` would cause some instrumentation
				labels to be lost (like `component_id`).
				"""
			pr_numbers: [11856]
		},
		{
			type: "fix"
			scopes: ["prometheus_exporter sink"]
			description: """
				Escape quotes and backslashes in metric tags for the `prometheus_exporter` sink. Previously these were
				not escaped and so resulted in invalid Prometheus export output that could not be scraped.
				"""
			pr_numbers: [11858]
		},
		{
			type: "fix"
			scopes: ["releasing"]
			description: """
				The Debian package names were updated to confirm to the [Debian packaging
				standards](https://www.debian.org/doc/manuals/debian-faq/pkg-basics.en.html#pkgname) by changing from
				`vector-${VECTOR_VERSION}-${PLATFORM}.deb` to `vector_${VECTOR_VERSION}_${PLATFORM}-${REV}.deb`. `REV`
				is typically `1`.
				"""
			pr_numbers: [11887]
		},
		{
			type: "fix"
			scopes: ["kafka source"]
			description: """
				The `kafka` source now reads the incoming message as raw bytes rather than trying to deserialize it as
				a UTF-8 string.
				"""
			pr_numbers: [11903]
		},
		{
			type: "enhancement"
			scopes: ["blackhole sink"]
			description: """
				Reporting for the `blackhole` sink can now be disabled via setting `print_interval_secs` to `0`.
				"""
			pr_numbers: [11912]
		},
		{
			type: "fix"
			scopes: ["aws_sqs source", "delivery"]
			description: """
				End-to-end acknowledgements were fixed for the `aws_sqs` source which would previously only acknowledge
				the last message in each batch from SQS when acknowledgements were enabled.
				"""
			pr_numbers: [11911]
		},
		{
			type: "fix"
			scopes: ["aws_sqs source", "delivery"]
			description: """
				The `aws_sqs` source would previously acknowledge events in SQS even if it failed to push them to
				downstream components. This has been corrected.
				"""
			pr_numbers: [11945]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			description: """
				The VRL `parse_xml` function now correctly parses the node attributes for solo nodes which have no
				siblings. Previously these node attributes were dropped.
				"""
			pr_numbers: [11910]
		},
		{
			type: "enhancement"
			scopes: ["route transform"]
			description: """
				The `route` transform now has an `_unmatched` route that can be consumed to receive events that did not
				match any of the other defined routes.
				"""
			pr_numbers: [11875]
		},
		{
			type: "enhancement"
			scopes: ["vrl"]
			description: """
				Add `ip_ntop` and `ip_pton` VRL functions which can convert IPv6 addresses to and from their byte and
				string representations.
				"""
			pr_numbers: [11917]
		},
		{
			type: "enhancement"
			scopes: ["vrl"]
			description: """
				Add `is_empty` VRL function which returns whether the given object, array, or string is empty.
				"""
			pr_numbers: [9732]
		},
		{
			type: "enhancement"
			scopes: ["splunk_hec source"]
			description: """
				The `splunk_hec` source now accepts events on `/services/collector`. This route is an alias for
				`/services/collector/event`.
				"""
			pr_numbers: [11941]
		},
		{
			type: "enhancement"
			scopes: ["datadog_metrics sink"]
			description: """
				The `datadog_metrics` sink now allows configuration of TLS via the standard `tls` options.
				"""
			pr_numbers: [11955]
		},
		{
			type: "fix"
			scopes: ["observability"]
			description: """
				`vector top` now reports error metrics correctly again.
				"""
			pr_numbers: [11973]
		},
		{
			type: "enhancement"
			scopes: ["aws_ec2_metadata transform"]
			description: """
				The `aws_ec2_metadata` transform now allows fetching the `account-id` field (this field must be opted
					into).
				"""
			pr_numbers: [11943]
		},
		{
			type: "fix"
			scopes: ["buffers", "observability"]
			breaking: true
			description: """
				The buffer received event metrics (`buffer_received_events_total` and `buffer_received_bytes_total`) now
				include the counts from discarded events. This was done to match the component metrics and to support
				future buffer `on_full` modes which may not discard events right away.
				"""
			pr_numbers: [11943, 12170]
		},
		{
			type: "enhancement"
			scopes: ["aws_sqs source"]
			description: """
				Additional options have been added to the `aws_sqs` source:

				- `delete_message` to control whether messages are deleted after
				  processing. This is useful for testing out the source.
				- `visibility_timeout_secs` to control how long messages are locked for
				  before being rereleased to be processed again. Tuning this is useful for controlling how long
				  a message will be "sent" if a Vector instance crashes before deleting the message.

				These options mirror those that existed for the `aws_s3` source for its SQS configuration.
				"""
			pr_numbers: [11981]
		},
		{
			type: "enhancement"
			scopes: ["kubernetes_logs source"]
			description: """
				The `kubernetes_logs` source has been rewritten to use the community supported
				[`kube-rs`](https://github.com/kube-rs/kube-rs) library. We expect that this will resolve some long
				outstanding bugs with Vector ceasing to process container logs. It also adds support for Kubernetes
				authentication token rotation.

				See [the highlight](/highlights/2022-03-28-kube-for-kubernetes_logs) for more details.
				"""
			pr_numbers: [11714]
		},
		{
			type: "fix"
			scopes: ["socket source"]
			description: """
				The `socket` source when in `udp` mode would previously include the port of the remote address in the
				enriched `host` field. This differed from the `tcp` mode where only the host part of the remote address
				is enriched. Instead, this source now does not include the port in the enriched `host` field.

				However, the `socket` source now has a `port_key` that can be set to opt into enrichment of the remote
				peer port as part of the address.
				"""
			pr_numbers: [12031, 12032]
		},
		{
			type: "enhancement"
			scopes: ["networking"]
			description: """
				Component `proxy` configuration can now include username/password encoded into the URL of the proxy like
				`http://john:password@my.proxy.com`.
				"""
			pr_numbers: [12016]
		},
		{
			type: "enhancement"
			scopes: ["vrl"]
			description: """
				A `strlen` function was added to VRL to complement the `length` function. The `length` function, when
				given a string, returns the number of bytes in that string. The `strlen` function returns the number of
				characters.
				"""
			pr_numbers: [12030]
		},
		{
			type: "fix"
			scopes: ["docker platform", "releasing"]
			breaking: true
			description: """
				Vector's published docker images no longer include `VOLUME` declarations. Instead, users should provide
				a volume at runtime if they require one. This avoids the behavior of Vector creating a volume for its
				data directory even if it is unused.

				See the [upgrade guide](/highlights/2022-03-22-0-21-0-upgrade-guide#docker-volume) for more information.
				"""
			pr_numbers: [12047]
		},
		{
			type: "enhancement"
			scopes: ["loki sink"]
			description: """
				Users can now provide dynamic label names to the `loki` sink via a trailing wildcard. Example:

				```yaml
				labels:
					pod_labels_*: {{ kubernetes.pod_labels }}
				```

				This is similar to the promtail configuration of:

				```yaml
				- action: labelmap
				  regex: __meta_kubernetes_pod_label_(.+)
				  replacement: pod_labels_$1
				```
				"""
			pr_numbers: [12041]
		},
		{
			type: "enhancement"
			scopes: ["prometheus_scrape source"]
			description: """
				Users can now configure query parameters on the `prometheus_scrape` source that are sent to all
				configured `endpoint`s via the new `query` option. This is useful when using Vector with a federated
				Prometheus endpoint.
				"""
			pr_numbers: [12033]
		},
		{
			type: "enhancement"
			scopes: ["vector sink"]
			description: """
				The `vector sink` can now enable gzip compression by setting `compression` to `true`.
				"""
			pr_numbers: [12059]
		},
		{
			type: "enhancement"
			scopes: ["splunk_hec source"]
			description: """
				The `splunk_hec` source's healthcheck that is exposed at `/service/collector/health` no longer requires
				a HEC token. This matches the behavior of the Splunk forwarder and makes it easier to use with load
				balancers that cannot set this header.
				"""
			pr_numbers: [12098]
		},
		{
			type: "enhancement"
			scopes: ["sinks"]
			description: """
				The `batch.timeout` configuration on sinks can now be include fractional seconds.
				"""
			pr_numbers: [11812]
		},
	]

	commits: [
		{sha: "9e82026a2317b89e525da33cb83bfbade4f3ac36", date: "2022-02-10 16:24:04 UTC", description: "bump smallvec from 1.7.0 to 1.8.0", pr_number:                                                   11279, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "18415050b60b08197e8135b7659390256995e844", date: "2022-02-10 14:30:19 UTC", description: "Value unification part 1 - NotNan floats", pr_number:                                            11291, scopes: ["core"], type:                                   "chore", breaking_change:       false, author: "Nathan Fox", files_count:           28, insertions_count:   342, deletions_count:  137},
		{sha: "b568b4dd7d43a078046565ed4bfe92f395296998", date: "2022-02-11 06:08:10 UTC", description: "Add soak test for HTTP JSON decoding/encoding", pr_number:                                       11286, scopes: [], type:                                         "chore", breaking_change:       false, author: "Pablo Sichert", files_count:        5, insertions_count:    143, deletions_count:  0},
		{sha: "07320c4a9c4dd562548d32d72f4d359fbbf443b7", date: "2022-02-11 06:45:31 UTC", description: "use new `value` crate for type checking in VRL", pr_number:                                      11296, scopes: ["vrl"], type:                                    "chore", breaking_change:       false, author: "Jean Mertz", files_count:           168, insertions_count:  2543, deletions_count: 3834},
		{sha: "41e863eebab4f1c2b67d2c10405ff10ba11eabb1", date: "2022-02-11 01:07:11 UTC", description: "Value unification part 2 - Regex", pr_number:                                                    11295, scopes: ["core"], type:                                   "chore", breaking_change:       false, author: "Nathan Fox", files_count:           8, insertions_count:    96, deletions_count:   2},
		{sha: "bebddeb4b6eb1c3b2a01c9d6b7a6045d8622fdf5", date: "2022-02-11 09:02:04 UTC", description: "comply with component spec", pr_number:                                                          11104, scopes: ["mongodb_metrics source"], type:                 "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:       5, insertions_count:    114, deletions_count:  13},
		{sha: "5c30af8e197d5e5ff7574aab626275e6b490e476", date: "2022-02-11 00:49:19 UTC", description: "bump memmap2 from 0.5.2 to 0.5.3", pr_number:                                                    11299, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "6a464c241d99058cee14887853f3d47e038720d9", date: "2022-02-11 11:26:19 UTC", description: "comply with component spec", pr_number:                                                          11223, scopes: ["transforms"], type:                             "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:       27, insertions_count:   437, deletions_count:  524},
		{sha: "4b832163c72d20ee93cc759c84fafcf4eade6a2a", date: "2022-02-11 11:27:10 UTC", description: "bump EmbarkStudios/cargo-deny-action from 1.2.10 to 1.2.11", pr_number:                          11314, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "5e4db9de84473ea817048e204561eb54a4f025d8", date: "2022-02-11 03:46:37 UTC", description: "Tag environment images", pr_number:                                                              11316, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    15, deletions_count:   1},
		{sha: "3ad4d5e2113adf3d5b314839535a37442118c4ad", date: "2022-02-11 06:42:42 UTC", description: "bump rustyline from 9.0.0 to 9.1.2", pr_number:                                                  11323, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    6, deletions_count:    19},
		{sha: "6fdb6cf5b47c9d3bf7dade332eb8d2a922b269e7", date: "2022-02-11 06:43:31 UTC", description: "bump smallvec from 1.7.0 to 1.8.0", pr_number:                                                   11324, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "4578e754a5328cc5aaade06643ac029dcac88f0c", date: "2022-02-11 06:44:22 UTC", description: "bump test-case from 1.2.1 to 1.2.3", pr_number:                                                  11326, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "97d9a43c2eb0549ac319ef325eae3555b4e8d244", date: "2022-02-11 06:44:55 UTC", description: "bump trust-dns-proto from 0.20.3 to 0.20.4", pr_number:                                          11327, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "6dbaca991b26a8bbe70a38aac92cdde476e38cec", date: "2022-02-11 17:25:46 UTC", description: "bump hyper from 0.14.16 to 0.14.17", pr_number:                                                  11325, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    6, deletions_count:    6},
		{sha: "c7b2a146b07072dd64fe016973e8ab41e59387ea", date: "2022-02-12 04:05:40 UTC", description: "comply with component spec", pr_number:                                                          11122, scopes: ["postgresql_metrics source"], type:              "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:       4, insertions_count:    192, deletions_count:  111},
		{sha: "6d8a6a7835d668a46315aba4db865e95bf3f9502", date: "2022-02-12 09:01:26 UTC", description: "typo", pr_number:                                                                                11331, scopes: [], type:                                         "docs", breaking_change:        false, author: "Tshepang Lekhonkhobe", files_count: 1, insertions_count:    1, deletions_count:    1},
		{sha: "c1012422a1a0d5c5e71062dca61529a151b2910f", date: "2022-02-12 02:01:29 UTC", description: "bump actions/github-script from 5 to 6", pr_number:                                              11339, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:    5, deletions_count:    5},
		{sha: "d4dd7aa4b74378e262500539c6f3615383d37bc8", date: "2022-02-12 12:56:13 UTC", description: "typo", pr_number:                                                                                11328, scopes: [], type:                                         "docs", breaking_change:        false, author: "Tshepang Lekhonkhobe", files_count: 1, insertions_count:    1, deletions_count:    1},
		{sha: "16cf27b6bb8d229d27f54ea887b9600c928bf65b", date: "2022-02-12 05:57:08 UTC", description: "Allow health endpoint to return 503s when Vector is shutting down", pr_number:                   11183, scopes: ["api"], type:                                    "feat", breaking_change:        false, author: "Spencer Gilbert", files_count:      6, insertions_count:    108, deletions_count:  11},
		{sha: "f4b9fc69e7a224e45faccc1ed62fd70b1d0e5e78", date: "2022-02-12 11:07:23 UTC", description: "remove credentials_file from docs", pr_number:                                                   11272, scopes: ["aws_sqs source"], type:                         "docs", breaking_change:        false, author: "Stephen Wakely", files_count:       5, insertions_count:    80, deletions_count:   30},
		{sha: "328dd13684cb0cae822c28d340cd8e7ba764f347", date: "2022-02-12 12:36:46 UTC", description: "Use `BytesMut` instead of `Vec<u8>` in `HttpSink` related code", pr_number:                      11232, scopes: [], type:                                         "chore", breaking_change:       false, author: "Pablo Sichert", files_count:        45, insertions_count:   350, deletions_count:  236},
		{sha: "7e2ba840bc0f2b89ce7d2b7007dbc3ccd2b99f9b", date: "2022-02-12 03:51:30 UTC", description: "Clarify `error` tag in component spec", pr_number:                                               11317, scopes: ["internal docs"], type:                          "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    9, deletions_count:    3},
		{sha: "f027beaab362c60a335f850890e9d22b2d7e1109", date: "2022-02-12 07:14:11 UTC", description: "Value unification part 3 - Value crate", pr_number:                                              11318, scopes: ["core"], type:                                   "chore", breaking_change:       false, author: "Nathan Fox", files_count:           57, insertions_count:   926, deletions_count:  706},
		{sha: "4985a42564e09fbea3213862d85b3438b7b81e73", date: "2022-02-12 06:24:02 UTC", description: "move more sources to send_batch", pr_number:                                                     11315, scopes: ["performance"], type:                            "chore", breaking_change:       false, author: "Luke Steensen", files_count:        4, insertions_count:    45, deletions_count:   31},
		{sha: "979773a6a3f8c8bc753907406645895ee56aea2e", date: "2022-02-12 07:45:39 UTC", description: "Error on nonexistent extract_from, no_outputs_from targets", pr_number:                          11340, scopes: ["unit tests"], type:                             "fix", breaking_change:         false, author: "Will", files_count:                 7, insertions_count:    128, deletions_count:  15},
		{sha: "e99e549828eede7e0e64d532c9b1c18a1ef34d4a", date: "2022-02-12 07:31:48 UTC", description: "Fix `check-docs.sh` compatibility with MacOS", pr_number:                                        11343, scopes: ["external docs"], type:                          "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        1, insertions_count:    1, deletions_count:    1},
		{sha: "44304c55f502408d5b1bd1b513c14209613f6fc6", date: "2022-02-15 08:15:59 UTC", description: "comply with component spec", pr_number:                                                          11124, scopes: ["socket source"], type:                          "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:       17, insertions_count:   140, deletions_count:  150},
		{sha: "6aa0f4c8fcb4db42706ba00dab03e245d2ce4059", date: "2022-02-14 23:18:10 UTC", description: "bump serde_json from 1.0.78 to 1.0.79", pr_number:                                               11354, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      9, insertions_count:    10, deletions_count:   10},
		{sha: "3375987100381cd15f6916136e4199cd5ea6d466", date: "2022-02-14 23:18:34 UTC", description: "bump hdrhistogram from 7.4.0 to 7.5.0", pr_number:                                               11355, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "34e08f6cd0aad7dbc66f60879f3937a4695cbee6", date: "2022-02-15 09:55:10 UTC", description: "Typo in 2020-08-31-mpl-2-0-license.md", pr_number:                                               11351, scopes: [], type:                                         "docs", breaking_change:        false, author: "Tshepang Lekhonkhobe", files_count: 1, insertions_count:    2, deletions_count:    2},
		{sha: "72a0ed6e4aafba091ee263554496edc3c39c83da", date: "2022-02-15 00:49:49 UTC", description: "bump rust_decimal from 1.19.0 to 1.21.0", pr_number:                                             11278, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "94d6e570842941453e2990ac69fd72614b4f7001", date: "2022-02-15 03:09:19 UTC", description: "emit splunk_hec metrics per request", pr_number:                                                 11289, scopes: ["performance"], type:                            "chore", breaking_change:       false, author: "Luke Steensen", files_count:        3, insertions_count:    31, deletions_count:   62},
		{sha: "9ce58871288f427889aecf0ce94cc12eac670d65", date: "2022-02-15 10:19:43 UTC", description: "bump EmbarkStudios/cargo-deny-action from 1.2.11 to 1.2.12", pr_number:                          11362, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "b878bf367669e500724e33278afd94c731ae1437", date: "2022-02-15 11:34:54 UTC", description: "fix prometheus_scrape warnings typo", pr_number:                                                 11357, scopes: [], type:                                         "docs", breaking_change:        false, author: "Ali Reza", files_count:             1, insertions_count:    1, deletions_count:    1},
		{sha: "690c683d9816efe0d5e5f1fe8b3d6b49a3db6547", date: "2022-02-15 05:35:40 UTC", description: "Value unification part 4 - VRL cleanup", pr_number:                                              11347, scopes: ["core"], type:                                   "chore", breaking_change:       false, author: "Nathan Fox", files_count:           81, insertions_count:   550, deletions_count:  430},
		{sha: "91cce99523aa6207bdc224e888ef7dd606ab8bc2", date: "2022-02-15 10:18:26 UTC", description: "Send specific event types into `SourceSender`", pr_number:                                       11366, scopes: ["sources"], type:                                "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        29, insertions_count:   173, deletions_count:  223},
		{sha: "015a707c6bb59c488d6892e1c01cd863fb02ea70", date: "2022-02-15 14:58:37 UTC", description: "Remove `value` tag from metrics", pr_number:                                                     11363, scopes: ["json_parser transform"], type:                  "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        1, insertions_count:    0, deletions_count:    2},
		{sha: "f81ebd4a24aa6c218e47d82eb68243699d02ed0e", date: "2022-02-15 23:20:31 UTC", description: "bump infer from 0.6.0 to 0.7.0", pr_number:                                                      11379, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    4, deletions_count:    4},
		{sha: "84e481b2fc3c0d30018af1528cdc1cadf805abd4", date: "2022-02-16 08:40:05 UTC", description: "bump libc from 0.2.117 to 0.2.118", pr_number:                                                   11388, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "b7cefd29c3a69542fde12e849325328132ba2b7c", date: "2022-02-16 01:25:05 UTC", description: "Retypist changes", pr_number:                                                                    11353, scopes: [], type:                                         "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   96, insertions_count:   210, deletions_count:  216},
		{sha: "d892d3597a96ca702b657db7cdff6b456997da72", date: "2022-02-16 11:11:56 UTC", description: "add schema definitions for decoding formats", pr_number:                                         11277, scopes: ["schemas"], type:                                "chore", breaking_change:       false, author: "Jean Mertz", files_count:           11, insertions_count:   391, deletions_count:  26},
		{sha: "18431051cc263c9333821672675e5e88da631f3f", date: "2022-02-16 06:53:32 UTC", description: "Remove deprecated subscriptions and deprecate unnecessary subscriptions", pr_number:             11364, scopes: ["api"], type:                                    "chore", breaking_change:       false, author: "Will", files_count:                 10, insertions_count:   64, deletions_count:   817},
		{sha: "e09cdbaab592ab48a104baf9b0f4d88cf1200ce3", date: "2022-02-16 04:35:44 UTC", description: "Migrate issue templates to GitHub forms", pr_number:                                             11378, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        6, insertions_count:    188, deletions_count:  255},
		{sha: "ef809ee61ecebd6a10ab8af7cd675efdced7533f", date: "2022-02-16 05:25:57 UTC", description: "Fix validation errors for GitHub issue templates", pr_number:                                    11403, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        2, insertions_count:    3, deletions_count:    6},
		{sha: "ec89912e95807cfd248e7281a64604b45bdfa8de", date: "2022-02-16 08:36:45 UTC", description: "Update upgrade guide and docs with Splunk channel behavior", pr_number:                          11394, scopes: ["splunk_hec sink"], type:                        "chore", breaking_change:       false, author: "Will", files_count:                 3, insertions_count:    41, deletions_count:   0},
		{sha: "c39e472b911f40b64feecdf4fd02f1756a3e0f9d", date: "2022-02-16 05:39:24 UTC", description: "Improve issue template community note", pr_number:                                               11406, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        2, insertions_count:    2, deletions_count:    4},
		{sha: "96b33c75dbb5ebc3708ae6cdfe3a81bd5b8fccfd", date: "2022-02-16 09:02:12 UTC", description: "Make TLS options configurable for AWS sinks", pr_number:                                         10314, scopes: ["sinks"], type:                                  "enhancement", breaking_change: false, author: "Chin-Ying Li", files_count:         19, insertions_count:   87, deletions_count:   15},
		{sha: "719abf65b93dca268bc4f5f6cfce6a5e9b0ac1c0", date: "2022-02-16 06:49:05 UTC", description: "enable blank issues", pr_number:                                                                 11404, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        2, insertions_count:    2, deletions_count:    2},
		{sha: "33b49b632e8edd6428eadc8ca1157dc73d7957a2", date: "2022-02-16 08:25:39 UTC", description: "Lower soak noise threshold", pr_number:                                                          11396, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        2, insertions_count:    4, deletions_count:    4},
		{sha: "e3c58177c13f48f267585219aed1b5c05343cfe6", date: "2022-02-16 09:49:34 UTC", description: "Update kubernetes manifests", pr_number:                                                         11408, scopes: ["releasing"], type:                              "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        17, insertions_count:   21, deletions_count:   21},
		{sha: "93c08b3f5390951dfa8c598062458c4a7dc9cdbb", date: "2022-02-17 02:16:53 UTC", description: "trace data type", pr_number:                                                                     10483, scopes: ["core"], type:                                   "feat", breaking_change:        false, author: "Pierre Rognant", files_count:       52, insertions_count:   649, deletions_count:  193},
		{sha: "f92d372bae2ae683ce9e2dcc06ae4387530c0458", date: "2022-02-17 05:09:00 UTC", description: "add schema definition", pr_number:                                                               11310, scopes: ["schemas", "datadog_agent source"], type:        "chore", breaking_change:       false, author: "Jean Mertz", files_count:           5, insertions_count:    460, deletions_count:  14},
		{sha: "483de3c0a51fc1ccdb25b5649800bd615ac1a2c9", date: "2022-02-17 05:29:52 UTC", description: "update `fn target_type_def` to return `Kind`", pr_number:                                        11336, scopes: ["schemas", "vrl"], type:                         "chore", breaking_change:       false, author: "Jean Mertz", files_count:           2, insertions_count:    11, deletions_count:   7},
		{sha: "d2ed308f013f73a328062ae2132f88e1dfbb1050", date: "2022-02-17 00:04:31 UTC", description: "bump rkyv from 0.7.31 to 0.7.32", pr_number:                                                     11413, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    17, deletions_count:   8},
		{sha: "c13b4a45222b9997cdc1f64c420f6bc3e1c08c7a", date: "2022-02-17 00:04:45 UTC", description: "bump async-graphql from 3.0.29 to 3.0.30", pr_number:                                            11414, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:    11, deletions_count:   11},
		{sha: "681dbb0bc1118d16c3ea66f302464e69e29e1274", date: "2022-02-17 00:05:18 UTC", description: "bump bitmask-enum from 1.1.2 to 1.1.3", pr_number:                                               11415, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:    4, deletions_count:    4},
		{sha: "01aedf0bb5acecf2b8983040e9f0d28ab07b1c82", date: "2022-02-17 02:04:29 UTC", description: "Allow empty pipelines", pr_number:                                                               11417, scopes: ["pipelines"], type:                              "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        2, insertions_count:    2, deletions_count:    5},
		{sha: "4d8436b73410f947d5972d4cb74904cfa7066721", date: "2022-02-17 02:57:43 UTC", description: "Re-order soak results", pr_number:                                                               11395, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    4, deletions_count:    4},
		{sha: "30b89abdccc206d11cac80bd81670de9a54763dc", date: "2022-02-17 07:07:19 UTC", description: "fix vector-core feature flags", pr_number:                                                       11421, scopes: [], type:                                         "chore", breaking_change:       false, author: "Luke Steensen", files_count:        4, insertions_count:    9, deletions_count:    9},
		{sha: "a1deaada0f6414665b32ec9bc2dcaa17fb0849b1", date: "2022-02-17 05:12:39 UTC", description: "Remove xtrace from soaks/bin/run_experiment", pr_number:                                         11179, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    1, deletions_count:    1},
		{sha: "ad4ab0b98a9dada8fd475a59c1aa3a413fa2662b", date: "2022-02-17 08:49:10 UTC", description: "remove legacy_lookup", pr_number:                                                                11427, scopes: ["core"], type:                                   "chore", breaking_change:       false, author: "Nathan Fox", files_count:           7, insertions_count:    2, deletions_count:    845},
		{sha: "80f44622e89376cf868826ec68009cdfebcd9444", date: "2022-02-17 15:58:42 UTC", description: "integrate event schema definitions", pr_number:                                                  11344, scopes: ["schemas", "topology"], type:                    "chore", breaking_change:       false, author: "Jean Mertz", files_count:           35, insertions_count:   356, deletions_count:  55},
		{sha: "502a3e7591fdbd2ed65be8419be873aa4568b94e", date: "2022-02-18 04:42:15 UTC", description: "hook up transforms to schema support", pr_number:                                                11367, scopes: ["schemas", "topology", "remap transform"], type: "chore", breaking_change:       false, author: "Jean Mertz", files_count:           7, insertions_count:    352, deletions_count:  123},
		{sha: "dc09e04135d86e149e0ff0a9853793445e73e36f", date: "2022-02-18 06:14:04 UTC", description: "add unix flag to any_open", pr_number:                                                           11439, scopes: ["ci"], type:                                     "fix", breaking_change:         false, author: "Stephen Wakely", files_count:       1, insertions_count:    1, deletions_count:    1},
		{sha: "f9b802b60366011769284005ebdd160d8544d247", date: "2022-02-17 23:17:50 UTC", description: "Update the http_datadog_filter_blackhole configuration", pr_number:                              11407, scopes: ["performance"], type:                            "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        2, insertions_count:    203, deletions_count:  225},
		{sha: "c13c0877c9dd56578e685ddf2f1d276e293591cf", date: "2022-02-17 23:25:36 UTC", description: "bump tower from 0.4.11 to 0.4.12", pr_number:                                                    11435, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    33, deletions_count:   20},
		{sha: "b20467baed91099e767489296dfc9dd9673b08b8", date: "2022-02-17 23:36:06 UTC", description: "bump rand from 0.8.4 to 0.8.5", pr_number:                                                       11360, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      6, insertions_count:    35, deletions_count:   45},
		{sha: "71bdb05ab7123b0c6162b1fdc0427f3de1a7b4ca", date: "2022-02-17 23:37:03 UTC", description: "bump async-graphql-warp from 3.0.29 to 3.0.30", pr_number:                                       11430, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "24f47ad8387b781fe8863717be33058a759d0227", date: "2022-02-18 09:48:09 UTC", description: "bump docker/login-action from 1.12.0 to 1.13.0", pr_number:                                      11450, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      5, insertions_count:    7, deletions_count:    7},
		{sha: "753fbde0a5cc913f4b536a3e21f9c39107965f14", date: "2022-02-18 10:18:01 UTC", description: "bump async-graphql from 3.0.30 to 3.0.31", pr_number:                                            11445, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:    12, deletions_count:   12},
		{sha: "79b8b24cb34aa063d6475945fae6e8ba776d44d2", date: "2022-02-18 10:35:13 UTC", description: "bump md-5 from 0.10.0 to 0.10.1", pr_number:                                                     11443, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    12, deletions_count:   12},
		{sha: "ce4bbe879865a69a89135bf9f832deb294d472d0", date: "2022-02-18 10:42:18 UTC", description: "bump sha2 from 0.10.1 to 0.10.2", pr_number:                                                     11444, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    4, deletions_count:    4},
		{sha: "e66cd2ab12a7981cf3cc62dfc10ef6f0e3268e63", date: "2022-02-18 11:44:45 UTC", description: "bump clap from 3.0.14 to 3.1.0", pr_number:                                                      11429, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:    6, deletions_count:    6},
		{sha: "f1bbee4272be0bf2d622ba286049c5f4045234d5", date: "2022-02-18 12:13:56 UTC", description: "bump async-graphql-warp from 3.0.30 to 3.0.31", pr_number:                                       11452, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "c4477c6cf6da800bc324654c04850f5558696705", date: "2022-02-18 07:46:20 UTC", description: "Value unification part 5 - the merge", pr_number:                                                11365, scopes: ["core"], type:                                   "chore", breaking_change:       false, author: "Nathan Fox", files_count:           43, insertions_count:   1127, deletions_count: 1441},
		{sha: "a926dae077cd18277a520c0ff083ea2faf7b0e60", date: "2022-02-18 07:41:53 UTC", description: "Add acknowledgement configuration to sinks", pr_number:                                          11346, scopes: ["sinks"], type:                                  "enhancement", breaking_change: false, author: "Bruce Guenter", files_count:        65, insertions_count:   436, deletions_count:  72},
		{sha: "a08614bbbb7987289d79692b2f98a3256f51c706", date: "2022-02-18 15:02:39 UTC", description: "fix datadog agent integration tests", pr_number:                                                 11456, scopes: ["tests"], type:                                  "chore", breaking_change:       false, author: "Jean Mertz", files_count:           1, insertions_count:    8, deletions_count:    2},
		{sha: "3596e310e6abe27a70c5d88deaf6d90c8c94637a", date: "2022-02-18 09:29:05 UTC", description: "Run cargo hack on each workspace member and all targets", pr_number:                             11419, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        23, insertions_count:   469, deletions_count:  479},
		{sha: "7d0720125f4318d8ea0e4ea453c8422ae32d53a2", date: "2022-02-18 11:08:23 UTC", description: "Revert run cargo hack on each workspace member and all targets", pr_number:                      11462, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        23, insertions_count:   479, deletions_count:  469},
		{sha: "67420a8898305f8ad72ffa3fea0eb9428a15bfc5", date: "2022-02-19 07:16:12 UTC", description: "add cache to recursive schema definition merging", pr_number:                                    11469, scopes: ["schemas"], type:                                "chore", breaking_change:       false, author: "Jean Mertz", files_count:           1, insertions_count:    20, deletions_count:   1},
		{sha: "ed89a1ef3c1ca1b0879b5df70a63f31a370a4b57", date: "2022-02-18 23:31:19 UTC", description: "bump tracing-subscriber from 0.3.8 to 0.3.9", pr_number:                                         11460, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      6, insertions_count:    8, deletions_count:    8},
		{sha: "a290c9453d4dc1a98264eb194f2e37b819a27da4", date: "2022-02-18 23:31:32 UTC", description: "bump tracing from 0.1.30 to 0.1.31", pr_number:                                                  11461, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      8, insertions_count:    42, deletions_count:   42},
		{sha: "840dc5f67200372fad5359fcc13e8037547779a6", date: "2022-02-18 23:31:46 UTC", description: "bump uaparser from 0.5.0 to 0.5.1", pr_number:                                                   11464, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "9d9b3d1f7da48b011b2767aad13295b438850c7b", date: "2022-02-18 23:31:59 UTC", description: "bump indoc from 1.0.3 to 1.0.4", pr_number:                                                      11465, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      6, insertions_count:    9, deletions_count:    9},
		{sha: "915199abd63639c3a7925cc61d451374cafcc1ec", date: "2022-02-19 09:15:34 UTC", description: "bump tokio from 1.16.1 to 1.17.0", pr_number:                                                    11428, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      9, insertions_count:    27, deletions_count:   13},
		{sha: "e6cabb6274f8d16ab189b5b6d01f5fafa8246de1", date: "2022-02-19 01:39:46 UTC", description: "Disallow unknown fields for hooks", pr_number:                                                   11459, scopes: ["lua transform"], type:                          "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        1, insertions_count:    1, deletions_count:    0},
		{sha: "920f038bd7b301ac2ce61356fd1950fab55fa051", date: "2022-02-19 11:48:48 UTC", description: "rename ElasticSearch to Elasticsearch", pr_number:                                               11437, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:       16, insertions_count:   152, deletions_count:  152},
		{sha: "65b9ce181a48765bf3e6bf65291326f3f6534ad3", date: "2022-02-19 11:50:02 UTC", description: "comply with component spec", pr_number:                                                          11125, scopes: ["statsd source"], type:                          "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:       3, insertions_count:    78, deletions_count:   23},
		{sha: "621d6b13a1a7a1ddde88351713b4514a5a7bb309", date: "2022-02-19 04:10:33 UTC", description: "Run cargo hack on each workspace member and all targets", pr_number:                             11463, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        24, insertions_count:   470, deletions_count:  480},
		{sha: "e665550bded5a4ac3bd962115e8f122db738d531", date: "2022-02-19 05:07:22 UTC", description: "Avoid using registry for soak images", pr_number:                                                11474, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        4, insertions_count:    62, deletions_count:   61},
		{sha: "8dbb127af56ecc1a7109b650454816e218e4eddc", date: "2022-02-19 06:02:44 UTC", description: "[retypist] `src/topology`", pr_number:                                                           11453, scopes: [], type:                                         "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   4, insertions_count:    19, deletions_count:   22},
		{sha: "fb445804144109ab693d65bdd367cd054a1cd39f", date: "2022-02-22 05:43:31 UTC", description: "Support tapping component inputs with vector tap", pr_number:                                    11293, scopes: ["api"], type:                                    "enhancement", breaking_change: false, author: "Will", files_count:                 11, insertions_count:   798, deletions_count:  63},
		{sha: "51ee56ea2577a0078effa82b79947fbc5eb1b855", date: "2022-02-22 17:37:21 UTC", description: "Add --quiet option to vector tap", pr_number:                                                    11321, scopes: ["api"], type:                                    "chore", breaking_change:       false, author: "Will", files_count:                 5, insertions_count:    46, deletions_count:   22},
		{sha: "13cbd68fcdd499b3effa87ca994431fd07d85e6f", date: "2022-02-23 01:07:31 UTC", description: "bump anyhow from 1.0.53 to 1.0.55", pr_number:                                                   11499, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "6a2850c26f2049c436610e0e260aa9fda82ea871", date: "2022-02-23 01:08:05 UTC", description: "bump strum from 0.23.0 to 0.24.0", pr_number:                                                    11498, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    4, deletions_count:    4},
		{sha: "40dd1148c79a5f298139c695b81d5bd2b4727527", date: "2022-02-23 03:10:25 UTC", description: "emit metric with good count for tcp", pr_number:                                                 11490, scopes: ["socket source"], type:                          "fix", breaking_change:         false, author: "Jérémie Drouet", files_count:       1, insertions_count:    3, deletions_count:    3},
		{sha: "ed4a41aaa32ae85adbf51ccdd1917784f07e217c", date: "2022-02-23 03:43:12 UTC", description: "move internal events error types to const", pr_number:                                           11334, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:       38, insertions_count:   767, deletions_count:  451},
		{sha: "a36b9a8cda77a1fb07df69bb7b5c8adbc7f58668", date: "2022-02-23 06:12:26 UTC", description: "modify schema definitions using VRL compiler", pr_number:                                        11384, scopes: ["schemas", "remap transform"], type:             "chore", breaking_change:       false, author: "Jean Mertz", files_count:           51, insertions_count:   429, deletions_count:  109},
		{sha: "530243cc30f427a852cfc1d69defab552c7ef705", date: "2022-02-22 23:23:44 UTC", description: "bump webbrowser from 0.5.5 to 0.6.0", pr_number:                                                 11480, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    99, deletions_count:   8},
		{sha: "4c88712c526e8cb7885fd1912c19dcc2b5342fc1", date: "2022-02-22 23:24:16 UTC", description: "bump console-subscriber from 0.1.2 to 0.1.3", pr_number:                                         11481, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    5, deletions_count:    3},
		{sha: "270f39c47b877ddf9ada7175dec48ccb5a873bb1", date: "2022-02-22 23:24:36 UTC", description: "bump libc from 0.2.118 to 0.2.119", pr_number:                                                   11482, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "18a8bed976e61c2dfae4c0a2e701e1e239edbeb1", date: "2022-02-22 23:24:58 UTC", description: "bump rust_decimal from 1.21.0 to 1.22.0", pr_number:                                             11483, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "cd00bc0f1ab216e2ab2c287155176443d68314b7", date: "2022-02-23 07:33:48 UTC", description: "Replace `structopt` with `clap` 3", pr_number:                                                   11470, scopes: ["cli"], type:                                    "chore", breaking_change:       false, author: "Lee Benson", files_count:           19, insertions_count:   222, deletions_count:  184},
		{sha: "7eda9071d706d384ce02cb11e3d3e04a70dec0c0", date: "2022-02-23 00:20:38 UTC", description: "revert modify schema definitions using VRL compiler", pr_number:                                 11513, scopes: ["schemas", "remap transform"], type:             "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        51, insertions_count:   109, deletions_count:  429},
		{sha: "0f1f909578f45cabf21f1d54458e385705e03ba5", date: "2022-02-23 03:10:26 UTC", description: "Remove unused dependabot reviewers and assignees", pr_number:                                    11511, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    0, deletions_count:    4},
		{sha: "fd06c17bcac4ec112a7dedfa40a025904d84dcb2", date: "2022-02-23 12:45:13 UTC", description: "Reduce VRL core such that it doesn't have any other dependencies to VRL", pr_number:             11492, scopes: ["vrl"], type:                                    "chore", breaking_change:       false, author: "Pablo Sichert", files_count:        36, insertions_count:   651, deletions_count:  263},
		{sha: "07d91c9015286865bab1f96075924066cc905255", date: "2022-02-23 07:05:45 UTC", description: "bump tikv-jemallocator from 0.4.1 to 0.4.3", pr_number:                                          11510, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:    4, deletions_count:    4},
		{sha: "20097ad492cc482a5099dddaa2067b386d0da7a0", date: "2022-02-23 07:06:32 UTC", description: "bump prettydiff from 0.6.0 to 0.6.1", pr_number:                                                 11508, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "22632cae0aad2408d679bb5b0ccd8ce7567a4a87", date: "2022-02-23 07:09:20 UTC", description: "bump rkyv from 0.7.32 to 0.7.33", pr_number:                                                     11485, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    5, deletions_count:    5},
		{sha: "be6a72ae4f757f438e00c0b2e800fded1b3ffa26", date: "2022-02-23 07:09:46 UTC", description: "bump semver from 1.0.5 to 1.0.6", pr_number:                                                     11501, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    5, deletions_count:    5},
		{sha: "af2a50e049ad14c57ecd2f7ed89a140a3ce880e4", date: "2022-02-23 07:15:02 UTC", description: "bump clap from 3.1.0 to 3.1.1", pr_number:                                                       11502, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      5, insertions_count:    10, deletions_count:   10},
		{sha: "c9fa288bc271cccc2fb364152c0a0df5b9aa39d8", date: "2022-02-23 08:01:07 UTC", description: "Fix cleanup make target", pr_number:                                                             11515, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    3, deletions_count:    3},
		{sha: "30d789a0b878a308608ae830a8a89d57a143ede7", date: "2022-02-23 16:42:54 UTC", description: "bump hyper-openssl from 0.9.1 to 0.9.2", pr_number:                                              11484, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    4, deletions_count:    4},
		{sha: "07401cdc939c17de5e1758f9a3bea7c6fcb2ee80", date: "2022-02-23 12:33:06 UTC", description: "update disk buffer impls to support multiple events via `EventCount`", pr_number:                11454, scopes: ["buffers"], type:                                "chore", breaking_change:       false, author: "Toby Lawrence", files_count:        62, insertions_count:   5078, deletions_count: 2448},
		{sha: "d5af463a65b418f4234dd62c00c0d6dc10f3a6e1", date: "2022-02-24 00:58:19 UTC", description: "break source when reader fails deserializing", pr_number:                                        11487, scopes: ["sources docker_logs"], type:                    "fix", breaking_change:         false, author: "Jérémie Drouet", files_count:       1, insertions_count:    7, deletions_count:    1},
		{sha: "681a872c3aeadae2f70879661554607272ce1bdf", date: "2022-02-23 23:01:35 UTC", description: "bump aws-types from 0.6.0 to 0.7.0", pr_number:                                                  11507, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:    40, deletions_count:   39},
		{sha: "3a9843cba48c8de3b2450c294eed3898dbcb9cfd", date: "2022-02-23 23:52:48 UTC", description: "bump strum_macros from 0.23.1 to 0.24.0", pr_number:                                             11519, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:    7, deletions_count:    7},
		{sha: "7ecd0d186093c22a44ab71fa5ffdead79e6b5df8", date: "2022-02-24 05:21:33 UTC", description: "bump clap from 3.1.1 to 3.1.2", pr_number:                                                       11533, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      5, insertions_count:    10, deletions_count:   10},
		{sha: "5e1a845097210dd753bfe911221839c3014d299c", date: "2022-02-24 05:51:44 UTC", description: "Batch filter transform emissions", pr_number:                                                    11516, scopes: [], type:                                         "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   2, insertions_count:    23, deletions_count:   8},
		{sha: "8119ed5f7480ef3ba8de1fdea9f49534c2ebed07", date: "2022-02-24 06:59:38 UTC", description: "Adjust soak workflow PR comment", pr_number:                                                     11518, scopes: ["ci"], type:                                     "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        2, insertions_count:    58, deletions_count:   0},
		{sha: "b42ec5aaa9679be697a11a08c694fa219e7111c3", date: "2022-02-25 00:56:57 UTC", description: "modify schema definitions using VRL compiler", pr_number:                                        11526, scopes: ["schemas", "remap transform"], type:             "chore", breaking_change:       false, author: "Jean Mertz", files_count:           51, insertions_count:   434, deletions_count:  117},
		{sha: "870de4a23e8178f132f84f4428f4f200fa192256", date: "2022-02-25 06:02:23 UTC", description: "use schema definition as input to compile VRL programs", pr_number:                              11385, scopes: ["remap transform", "schemas"], type:             "feat", breaking_change:        false, author: "Jean Mertz", files_count:           6, insertions_count:    56, deletions_count:   12},
		{sha: "cd1d718c28c90c337e1171e7815e32a69f498cae", date: "2022-02-25 07:33:04 UTC", description: "disable schema integration by default", pr_number:                                               11555, scopes: ["schemas"], type:                                "chore", breaking_change:       false, author: "Jean Mertz", files_count:           5, insertions_count:    36, deletions_count:   4},
		{sha: "32d9adb4cbc0ff1b571bc4e3ac105e8c6768aaf0", date: "2022-02-25 07:52:13 UTC", description: "correctly set schemas", pr_number:                                                               11556, scopes: ["schemas"], type:                                "chore", breaking_change:       false, author: "Jean Mertz", files_count:           1, insertions_count:    1, deletions_count:    1},
		{sha: "bc914664955e464f5d49b677d4d1009de8467047", date: "2022-02-25 04:40:07 UTC", description: "Update from async_nats to nats::asynk", pr_number:                                               11542, scopes: ["nats"], type:                                   "chore", breaking_change:       false, author: "Adit Sachde", files_count:          4, insertions_count:    69, deletions_count:   39},
		{sha: "11c545c35ef46a6f8029d50a645232fcad80ff3b", date: "2022-02-25 06:29:42 UTC", description: "bump human_bytes from 0.3.0 to 0.3.1", pr_number:                                                11523, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "2d4400448d7cb67ffcafd09f30b224f4a9697a37", date: "2022-02-25 06:34:14 UTC", description: "Fix Windows builds", pr_number:                                                                  11560, scopes: ["ci"], type:                                     "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        1, insertions_count:    7, deletions_count:    7},
		{sha: "7289922cfefac7ff67e62a64ef4c2170370a35fd", date: "2022-02-25 11:08:33 UTC", description: "Error if acknowledgements are enabled for an unsupported sink", pr_number:                       11449, scopes: ["config", "sinks"], type:                        "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        54, insertions_count:   274, deletions_count:  12},
		{sha: "cc960faabdc194783db35b5bd40e1eaa48f0616b", date: "2022-02-25 13:11:48 UTC", description: "Simplify trivial `async { X.await }` anti-pattern", pr_number:                                   11564, scopes: [], type:                                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        9, insertions_count:    29, deletions_count:   30},
		{sha: "26b4e959613dd1c6768f11ba2e1ff6e8df0494bd", date: "2022-02-25 12:01:23 UTC", description: "Apply deny unreachable_pub to src/conditions", pr_number:                                        11547, scopes: [], type:                                         "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   7, insertions_count:    26, deletions_count:   34},
		{sha: "1d9a34e24c416501c3f6377b8400253a3ad538f6", date: "2022-02-26 05:59:03 UTC", description: "remove schema registry indirection", pr_number:                                                  11431, scopes: ["schemas"], type:                                "chore", breaking_change:       false, author: "Jean Mertz", files_count:           20, insertions_count:   287, deletions_count:  343},
		{sha: "b8fb1e3888bc9de726d56f96863ab881a2162cae", date: "2022-02-26 00:48:51 UTC", description: "Prevent idle connections from hoarding permits ", pr_number:                                     11549, scopes: ["socket source"], type:                          "fix", breaking_change:         false, author: "Nathan Fox", files_count:           1, insertions_count:    8, deletions_count:    0},
		{sha: "6dd9d65c85368200434ef68ae76430e8f7dbe64e", date: "2022-02-26 00:17:53 UTC", description: "Send arrays of events through the topology", pr_number:                                          11072, scopes: ["topology"], type:                               "enhancement", breaking_change: false, author: "Bruce Guenter", files_count:        35, insertions_count:   1252, deletions_count: 318},
		{sha: "11811ce337ffa71777e5eccdd6e015192be27c10", date: "2022-02-26 00:33:43 UTC", description: "bump aws-types from 0.7.0 to 0.8.0", pr_number:                                                  11566, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    38, deletions_count:   38},
		{sha: "9855ea6a2ae9066cdbfa1fb3e9294b3a6f1a9176", date: "2022-02-26 05:14:11 UTC", description: "Add vector tap guide", pr_number:                                                                11517, scopes: ["guides website"], type:                         "docs", breaking_change:        false, author: "Will", files_count:                 2, insertions_count:    261, deletions_count:  1},
		{sha: "a1b72e22e5142ad8a821c5b9e943c5ed0bbaedd8", date: "2022-02-26 11:31:09 UTC", description: "allow the usage of `region` and `endpoint`", pr_number:                                          11578, scopes: ["aws_s3 sink"], type:                            "fix", breaking_change:         false, author: "Patrik", files_count:               2, insertions_count:    5, deletions_count:    4},
		{sha: "801ee2178b8e30d2695a131185e09b11c7623ffd", date: "2022-02-26 11:34:46 UTC", description: "Match capacity of `serde_json::to_vec`", pr_number:                                              11565, scopes: [], type:                                         "chore", breaking_change:       false, author: "Pablo Sichert", files_count:        1, insertions_count:    3, deletions_count:    1},
		{sha: "b587aec2b46632b84c465cba832d6b9ed0d314f7", date: "2022-02-26 05:10:25 UTC", description: "Fix test for `aws_s3` config", pr_number:                                                        11585, scopes: ["tests"], type:                                  "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        1, insertions_count:    2, deletions_count:    13},
		{sha: "89aa5f9fd07e37c5a6139e9275d5b8fae1d5e164", date: "2022-02-26 05:12:14 UTC", description: "bump test-case from 1.2.3 to 2.0.0", pr_number:                                                  11522, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    4, deletions_count:    4},
		{sha: "165c0d34d2d1c68958b48f2aac63164fe8cb056f", date: "2022-02-26 06:31:35 UTC", description: "Fix soak comment posting workflow", pr_number:                                                   11593, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    2, deletions_count:    2},
		{sha: "02f2458bb0e253a2f3f3d900d5a0131ef5d71f6a", date: "2022-02-26 06:32:12 UTC", description: "Remove WASM transform from docs", pr_number:                                                     11540, scopes: ["wasm transform"], type:                         "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        8, insertions_count:    4, deletions_count:    1094},
		{sha: "8a52e400043b66a180573b222c26c162ad6182ce", date: "2022-02-26 06:32:42 UTC", description: "Always render batch config options", pr_number:                                                  11530, scopes: ["sinks"], type:                                  "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        1, insertions_count:    21, deletions_count:   27},
		{sha: "13b40717bbdc7deac79cb0c243979fbddf63168b", date: "2022-02-26 09:15:51 UTC", description: "Inline `ServiceLogic` into batch sink framework", pr_number:                                     11569, scopes: ["sinks"], type:                                  "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        8, insertions_count:    55, deletions_count:   155},
		{sha: "6bf86b30563eed7a54c1b24829bff3f3c3011519", date: "2022-02-26 10:29:55 UTC", description: "Disambiguate `LogEvent` and `TraceEvent`", pr_number:                                            11584, scopes: ["core"], type:                                   "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        9, insertions_count:    249, deletions_count:  65},
		{sha: "71d067fc2b8f6bb056a98f388815d78cbd90cfb7", date: "2022-02-26 10:57:47 UTC", description: "Refine testing for needed env vars for datadog-agent integration tests", pr_number:              11457, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        7, insertions_count:    10, deletions_count:   10},
		{sha: "258c49aabfc093563f6c31a76d92e9556d5bf4bf", date: "2022-02-26 15:25:11 UTC", description: "Change vector top --human_metrics short flag", pr_number:                                        11581, scopes: ["api"], type:                                    "fix", breaking_change:         false, author: "Will", files_count:                 3, insertions_count:    11, deletions_count:   2},
		{sha: "a7e395b6a5edaac19e523388e9c92cee385a94f2", date: "2022-02-27 00:33:00 UTC", description: "Update prometheus version for integration tests", pr_number:                                     10462, scopes: ["tests"], type:                                  "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        2, insertions_count:    2, deletions_count:    3},
		{sha: "7556dc367c95d5cdaf08bbd6e700ed9750c9dc54", date: "2022-02-27 04:22:02 UTC", description: "Slim unused dependencies from the project", pr_number:                                           11541, scopes: [], type:                                         "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   7, insertions_count:    96, deletions_count:   134},
		{sha: "30d50deeb9aabdc79a1e7fbcd2805fa227f7e221", date: "2022-02-27 08:52:33 UTC", description: "Use more strict deny flags in the project", pr_number:                                           11597, scopes: [], type:                                         "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   39, insertions_count:   117, deletions_count:  45},
		{sha: "ac3c1396f8ffdfe2fda58f939a53cfec275bb1f5", date: "2022-02-27 12:01:37 UTC", description: "Remove trivial instances of getset use", pr_number:                                              11598, scopes: [], type:                                         "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   12, insertions_count:   658, deletions_count:  95},
		{sha: "a9e627cb8268290935feb9bf95c7a535b0b9c1df", date: "2022-02-28 23:35:33 UTC", description: "bump wiremock from 0.5.10 to 0.5.11", pr_number:                                                 11600, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    12, deletions_count:   33},
		{sha: "4627441f4724ded27d89ff4f92216dd767706207", date: "2022-02-28 23:42:38 UTC", description: "bump async-compression from 0.3.7 to 0.3.12", pr_number:                                         11601, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    9, deletions_count:    9},
		{sha: "482d60ad13c8677f1d87433831e927a515e2ba49", date: "2022-03-01 09:43:23 UTC", description: "small fixes/improvements", pr_number:                                                            11603, scopes: [], type:                                         "docs", breaking_change:        false, author: "Tshepang Lekhonkhobe", files_count: 1, insertions_count:    3, deletions_count:    3},
		{sha: "32caaadbb129c6cb612f7843c1d966b27f31af3e", date: "2022-03-01 08:45:08 UTC", description: "migrate kafka integration test", pr_number:                                                      10817, scopes: ["tests"], type:                                  "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:       13, insertions_count:   230, deletions_count:  140},
		{sha: "ef5b851b2965bb630bcaca5a022498f2e2291ab8", date: "2022-03-01 00:48:22 UTC", description: "Fix kafka integration test features", pr_number:                                                 11608, scopes: ["ci"], type:                                     "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        1, insertions_count:    1, deletions_count:    1},
		{sha: "762a5e623fb1cbdf6c81aec506c0f80cd6aee9db", date: "2022-03-01 04:03:19 UTC", description: "Remove instrumentation on LogEvent", pr_number:                                                  11592, scopes: ["core"], type:                                   "chore", breaking_change:       false, author: "Nathan Fox", files_count:           1, insertions_count:    2, deletions_count:    18},
		{sha: "8caf7faadd7292ce95a02fc780b182ef664490f3", date: "2022-03-01 01:49:46 UTC", description: "Apply clippy lints from 1.59.0", pr_number:                                                      11595, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        55, insertions_count:   115, deletions_count:  107},
		{sha: "c2e4fa14a9b9d362f817a0461789011f5950d3e6", date: "2022-03-01 11:37:22 UTC", description: "bump actions/setup-python from 2 to 3", pr_number:                                               11609, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    5, deletions_count:    5},
		{sha: "85bbb5e2191995e4ea4fe71e4ce3513daceaf4ec", date: "2022-03-01 11:44:51 UTC", description: "bump docker/login-action from 1.13.0 to 1.14.0", pr_number:                                      11610, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:    5, deletions_count:    5},
		{sha: "c5755daf30f54843027f68811ac81bd515410b7f", date: "2022-03-01 04:00:44 UTC", description: "Fix soak comment workflow", pr_number:                                                           11616, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    1, deletions_count:    1},
		{sha: "607cc93bdd1519fec87ac08cc9ba85514a68db2e", date: "2022-03-01 05:07:08 UTC", description: "Refine `error_code`", pr_number:                                                                 11612, scopes: [], type:                                         "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        1, insertions_count:    11, deletions_count:   6},
		{sha: "d81c6b7c4570b856c57d92c32b5f96f449a8a119", date: "2022-03-01 05:58:02 UTC", description: "Fix handling of whether to build image for soak.sh", pr_number:                                  11614, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    8, deletions_count:    7},
		{sha: "66c6e1ab5b23e99fa8619e778d640e3574163da8", date: "2022-03-01 06:11:05 UTC", description: "Another adjustment to soak comment workflow", pr_number:                                         11619, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    1, deletions_count:    1},
		{sha: "82264fa6c280fcee6cdaf0c95074f8c6ddaf3f8f", date: "2022-03-01 10:06:29 UTC", description: "Add --meta flag to vector tap", pr_number:                                                       11531, scopes: ["api"], type:                                    "enhancement", breaking_change: false, author: "Will", files_count:                 12, insertions_count:   339, deletions_count:  67},
		{sha: "6ed124a8bb208668ea8b8e825e054056807e3f4d", date: "2022-03-01 11:39:37 UTC", description: "Split config source into component modules", pr_number:                                          11620, scopes: ["core"], type:                                   "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        4, insertions_count:    441, deletions_count:  418},
		{sha: "de2b7153cb64b4fd862e6ae3799115dbbb6db2f4", date: "2022-03-02 02:38:32 UTC", description: "add missing events", pr_number:                                                                  11383, scopes: ["sinks"], type:                                  "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:       45, insertions_count:   719, deletions_count:  317},
		{sha: "ffe0b8394014622d60428e6c004850d70d5d40b1", date: "2022-03-02 04:41:27 UTC", description: "test the VM functions as well in the `test_function` macro.", pr_number:                         11546, scopes: ["vrl"], type:                                    "chore", breaking_change:       false, author: "Stephen Wakely", files_count:       5, insertions_count:    171, deletions_count:  4},
		{sha: "2a26deca19ea6c86bfbee18995b4b7c79c5612b7", date: "2022-03-02 05:59:11 UTC", description: "add criterion benchmarks for the VRL VM", pr_number:                                             11607, scopes: ["vrl"], type:                                    "chore", breaking_change:       false, author: "Stephen Wakely", files_count:       3, insertions_count:    94, deletions_count:   0},
		{sha: "2123bf551b0c8694a7ca3d0d348671f008ed0916", date: "2022-03-02 06:12:26 UTC", description: "update vrl-vm to be a runtime option.", pr_number:                                               11554, scopes: ["vrl"], type:                                    "feat", breaking_change:        false, author: "Stephen Wakely", files_count:       9, insertions_count:    137, deletions_count:  86},
		{sha: "ae2c86f8452d9bd89064543df601aca18bd835ec", date: "2022-03-01 23:15:43 UTC", description: "bump clap from 3.1.2 to 3.1.3", pr_number:                                                       11626, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      5, insertions_count:    10, deletions_count:   10},
		{sha: "080852b3faada3f9121db7b66be3e194fe2e4c04", date: "2022-03-01 23:16:27 UTC", description: "bump lru from 0.7.2 to 0.7.3", pr_number:                                                        11627, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "017efddce19c9d344d39f2556a08558832f1e7ee", date: "2022-03-02 09:19:30 UTC", description: "allow mutable access to external contexts", pr_number:                                           11438, scopes: ["vrl"], type:                                    "chore", breaking_change:       false, author: "Jean Mertz", files_count:           141, insertions_count:  340, deletions_count:  367},
		{sha: "7b606432607bf60035a3801fdafbf0ea6e3b79cd", date: "2022-03-02 02:31:58 UTC", description: "Fix handling of indexer acknowledgements config", pr_number:                                     11622, scopes: ["splunk_hec sink"], type:                        "fix", breaking_change:         false, author: "Bruce Guenter", files_count:        66, insertions_count:   533, deletions_count:  205},
		{sha: "635ddcc2258ad591005c0f9056858f4b87c2a8b4", date: "2022-03-02 01:27:57 UTC", description: "Use run_id rather than event number for soak artifacts", pr_number:                              11621, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        2, insertions_count:    33, deletions_count:   33},
		{sha: "27918cd8fa0ab3f8b4a7ad4b235e93b5f7807dff", date: "2022-03-02 02:22:21 UTC", description: "Fix artifact lookup in soak comment workflow", pr_number:                                        11632, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    1, deletions_count:    1},
		{sha: "89ca645cfd8822d78c175a69355bea787cdf5bfc", date: "2022-03-02 03:32:05 UTC", description: "bump trust-dns-proto from 0.20.4 to 0.21.1", pr_number:                                          11604, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      6, insertions_count:    234, deletions_count:  324},
		{sha: "9af949e10d0e4830700e6d94c713170f9bb65e34", date: "2022-03-02 06:59:48 UTC", description: "Remove instrumentation from TraceEvent", pr_number:                                              11618, scopes: ["core"], type:                                   "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        1, insertions_count:    0, deletions_count:    5},
		{sha: "c349b09cfa0da480c155ed9f90401ec341c729b4", date: "2022-03-02 08:18:34 UTC", description: "add architecture.md", pr_number:                                                                 11144, scopes: [], type:                                         "docs", breaking_change:        false, author: "Luke Steensen", files_count:        1, insertions_count:    204, deletions_count:  0},
		{sha: "9c566ba7c32a2809fbdf45942ef7fab729e4b53c", date: "2022-03-02 07:19:03 UTC", description: "pass along PR number to soak test comment", pr_number:                                           11635, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        2, insertions_count:    37, deletions_count:   1},
		{sha: "f39c3c63e42f7384dd3f641ece7512783e9e2c88", date: "2022-03-02 11:09:32 UTC", description: "Build soak builder for aarch64", pr_number:                                                      11636, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    10, deletions_count:   0},
		{sha: "32c04438b6514f8e1f12b4256cbb162ce38da7e9", date: "2022-03-03 04:39:09 UTC", description: "add `set_semantic_meaning` function", pr_number:                                                 11440, scopes: ["vrl", "schemas"], type:                         "feat", breaking_change:        false, author: "Jean Mertz", files_count:           4, insertions_count:    100, deletions_count:  0},
		{sha: "cf28c2b5dbb65b23e41429cfef7fb4fb5acec6f9", date: "2022-03-03 05:04:49 UTC", description: "integrate schema support into encoding formats", pr_number:                                      11505, scopes: ["schemas", "codecs"], type:                      "chore", breaking_change:       false, author: "Jean Mertz", files_count:           7, insertions_count:    80, deletions_count:   11},
		{sha: "7c21733d1aae74cce882b0397fa58f69811f584c", date: "2022-03-03 05:58:03 UTC", description: "bump mlua from 0.7.3 to 0.7.4", pr_number:                                                       11640, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:    5, deletions_count:    5},
		{sha: "92cd587d6115a1a58889d80967a520170d44f2bf", date: "2022-03-03 05:58:34 UTC", description: "bump roaring from 0.8.1 to 0.9.0", pr_number:                                                    11641, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    7, deletions_count:    7},
		{sha: "1b8db06505856e37771161310e32b2a1f58cfbb6", date: "2022-03-03 10:37:34 UTC", description: "bump actions/checkout from 2.4.0 to 3", pr_number:                                               11652, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      11, insertions_count:   67, deletions_count:   67},
		{sha: "5f58371a722068a67d341b04714f57f45a7b2e51", date: "2022-03-03 11:07:44 UTC", description: "bump actions/labeler from 3 to 4", pr_number:                                                    11653, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    1, deletions_count:    1},
		{sha: "b6a0b89d03680f104dd2aa6d5fc8fd426fe05d4b", date: "2022-03-03 12:20:46 UTC", description: "bump docker/login-action from 1.14.0 to 1.14.1", pr_number:                                      11651, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:    5, deletions_count:    5},
		{sha: "5cc6b85f151b74d04584aadc596ab05f7fcde3c7", date: "2022-03-03 07:28:33 UTC", description: "Update docs to reflect acknowledgements support", pr_number:                                     7436, scopes: ["external docs"], type:                           "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        83, insertions_count:   155, deletions_count:  29},
		{sha: "cdb8b38ef2e73e5084feacd19f5ee83822c066ad", date: "2022-03-03 05:34:32 UTC", description: "Fix link to logfmt", pr_number:                                                                  11659, scopes: [], type:                                         "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        1, insertions_count:    2, deletions_count:    2},
		{sha: "c61794c9259260024312ba052c844ba806be3d64", date: "2022-03-04 08:08:41 UTC", description: "initial trace support in `datadog_agent source", pr_number:                                      11033, scopes: ["datadog_agent source"], type:                   "feat", breaking_change:        false, author: "Pierre Rognant", files_count:       13, insertions_count:   1272, deletions_count: 634},
		{sha: "8d54ee33f17f0f8374a181494e75ca9f3b0ef8ff", date: "2022-03-04 02:16:27 UTC", description: "Warn on invalid matches in vector tap", pr_number:                                               11411, scopes: ["api"], type:                                    "chore", breaking_change:       false, author: "Will", files_count:                 9, insertions_count:    442, deletions_count:  118},
		{sha: "333628b8727b20771c853d34135d0763544b6493", date: "2022-03-03 23:58:04 UTC", description: "bump clap from 3.1.3 to 3.1.5", pr_number:                                                       11665, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      5, insertions_count:    15, deletions_count:   15},
		{sha: "277d4c002ac5ce55de3dd27fcaa2cc91c59b02b4", date: "2022-03-03 23:58:21 UTC", description: "bump termcolor from 1.1.2 to 1.1.3", pr_number:                                                  11666, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "0f191279a17fe56851449b9106da4235a5219e3c", date: "2022-03-04 03:58:50 UTC", description: "Add reconnect behavior to top and tap", pr_number:                                               11589, scopes: ["api"], type:                                    "enhancement", breaking_change: false, author: "Will", files_count:                 9, insertions_count:    335, deletions_count:  251},
		{sha: "f091f402aa6a51217ad91925e56cef18ebc60d04", date: "2022-03-04 10:46:48 UTC", description: "Use correct env var for Datadog integration test", pr_number:                                    11670, scopes: ["ci"], type:                                     "fix", breaking_change:         false, author: "Pierre Rognant", files_count:       2, insertions_count:    7, deletions_count:    3},
		{sha: "0db6214c54b55f8fc19cc5b0f4579e0802945e0d", date: "2022-03-04 08:15:34 UTC", description: "initial write offset sometimes calculated incorrectly in disk v1", pr_number:                    11590, scopes: ["buffers"], type:                                "fix", breaking_change:         false, author: "Toby Lawrence", files_count:        5, insertions_count:    391, deletions_count:  93},
		{sha: "9eef3acd3eb31dc254111eca4601033d91d72aab", date: "2022-03-04 05:43:07 UTC", description: "Fix clippy flag application", pr_number:                                                         11674, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    1, deletions_count:    1},
		{sha: "f56668332e0a423b23773adf9ad7d5d8a7216b39", date: "2022-03-04 22:17:32 UTC", description: "bump once_cell from 1.9.0 to 1.10.0", pr_number:                                                 11681, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      8, insertions_count:    10, deletions_count:   10},
		{sha: "59c1b53e0750b11ad8f0ab02303589f24c11e633", date: "2022-03-04 22:18:12 UTC", description: "bump rkyv from 0.7.33 to 0.7.35", pr_number:                                                     11682, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    5, deletions_count:    5},
		{sha: "5869ecd282ae391ac6ec322aadb01f1e600f84df", date: "2022-03-05 03:09:24 UTC", description: "Make remainder fallible", pr_number:                                                             11668, scopes: ["vrl"], type:                                    "fix", breaking_change:         true, author:  "Stephen Wakely", files_count:       4, insertions_count:    52, deletions_count:   4},
		{sha: "4016629c89fd6584674187d253910947f97126ad", date: "2022-03-04 23:51:45 UTC", description: "correct fix for topology reloading around rebuilt sinks", pr_number:                             11680, scopes: ["topology"], type:                               "fix", breaking_change:         false, author: "Luke Steensen", files_count:        3, insertions_count:    60, deletions_count:   29},
		{sha: "7372b4270208e827446be4f4f187d74813172eed", date: "2022-03-05 06:13:06 UTC", description: "Add `vector config` subcommand to output a normalized configuration", pr_number:                 11442, scopes: ["cli"], type:                                    "feat", breaking_change:        false, author: "Lee Benson", files_count:           10, insertions_count:   596, deletions_count:  467},
		{sha: "ae0ecad24fbff4352a9bf0b2ea3be84bb8ba0d07", date: "2022-03-04 23:54:34 UTC", description: "Let scrape_interval_secs have fractional seconds", pr_number:                                    11673, scopes: ["internal_metrics source"], type:                "feat", breaking_change:        false, author: "Jesse Szwedko", files_count:        6, insertions_count:    21, deletions_count:   17},
		{sha: "50a503f4d2c1bbc90131ec683cdcf9437601ee8f", date: "2022-03-05 02:32:19 UTC", description: "Fix cross-compilation", pr_number:                                                               11687, scopes: ["ci"], type:                                     "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        4, insertions_count:    11, deletions_count:   31},
		{sha: "36b4b482b56418ff2dbb5daeb76e5f919f8b8d1e", date: "2022-03-05 04:46:24 UTC", description: "Send arrays of events into the topology", pr_number:                                             11587, scopes: ["sources"], type:                                "enhancement", breaking_change: false, author: "Bruce Guenter", files_count:        23, insertions_count:   544, deletions_count:  254},
		{sha: "940132f8da1c9823c62643bf92b9d6d5671b4e40", date: "2022-03-05 07:02:37 UTC", description: "remove slim-build from k8s ci", pr_number:                                                       11689, scopes: [], type:                                         "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "d7566f980432d999ae9bf909d31222ae2fb1a9d0", date: "2022-03-05 04:11:20 UTC", description: "bump async-graphql from 3.0.31 to 3.0.33", pr_number:                                            11685, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:    11, deletions_count:   11},
		{sha: "7ec12cf5ff64bb427e606abb278be50b81adf89a", date: "2022-03-05 06:38:33 UTC", description: "Add timeout to batch read loop", pr_number:                                                      11671, scopes: ["journald source"], type:                        "enhancement", breaking_change: false, author: "Bruce Guenter", files_count:        1, insertions_count:    153, deletions_count:  105},
		{sha: "64ff2dec06f508b2e50bf68c75cb517260312e3c", date: "2022-03-05 07:42:54 UTC", description: "Add configurable unit for to_timestamp", pr_number:                                              11663, scopes: ["vrl"], type:                                    "feat", breaking_change:        false, author: "Spencer Gilbert", files_count:      2, insertions_count:    231, deletions_count:  32},
		{sha: "419da21d80615c6f3a472fb4c542bdf0c1eacfb7", date: "2022-03-05 07:11:05 UTC", description: "Fix topology test", pr_number:                                                                   11693, scopes: ["tests"], type:                                  "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        1, insertions_count:    4, deletions_count:    1},
		{sha: "920021daf3e41bfa8e7ba092a7ade7f6cbe674c5", date: "2022-03-05 09:46:11 UTC", description: "Drop `ready_arrays` adapter", pr_number:                                                         11695, scopes: ["topology"], type:                               "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        3, insertions_count:    2, deletions_count:    331},
		{sha: "1b6aea2ff6f98d5697643e7cafb188874271cae8", date: "2022-03-05 10:21:21 UTC", description: "Make more sources send batches of events", pr_number:                                            11594, scopes: ["sources"], type:                                "enhancement", breaking_change: false, author: "Bruce Guenter", files_count:        19, insertions_count:   81, deletions_count:   97},
		{sha: "2ddf714e51115d8dbcf4b9c5bbb1d8aae5eb3221", date: "2022-03-05 09:04:20 UTC", description: "Bring soak http_pipelines_blackhole_acks up-to-date with http_pipelines_blackhole", pr_number:   11696, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    1864, deletions_count: 1213},
		{sha: "11dddce5989d3c3bc4bbf731c44abd3e1fc1f55a", date: "2022-03-06 07:49:17 UTC", description: "Introduce a new route transform  benchmark", pr_number:                                          11697, scopes: [], type:                                         "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   5, insertions_count:    206, deletions_count:  10},
		{sha: "2ebf121bb68da28c452a1e27fa61b7377de8df41", date: "2022-03-08 08:03:11 UTC", description: "Refactor `util::http::HttpSink` to allow mutable access to `self` in `encode_event`", pr_number: 11628, scopes: [], type:                                         "chore", breaking_change:       false, author: "Pablo Sichert", files_count:        10, insertions_count:   269, deletions_count:  117},
		{sha: "2ad53d2c8c1c89ed1dcd1c615076f1f402e725dd", date: "2022-03-07 23:44:52 UTC", description: "bump num_enum from 0.5.6 to 0.5.7", pr_number:                                                   11690, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    5, deletions_count:    5},
		{sha: "b6bcf925b537c64d7c0282e61ed650ac20d7a1f3", date: "2022-03-07 23:46:03 UTC", description: "bump cidr-utils from 0.5.5 to 0.5.6", pr_number:                                                 11701, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "c1d089bdfbde5c6dac3e961d2738b248fd62fddf", date: "2022-03-08 01:38:14 UTC", description: "bump async-graphql from 3.0.33 to 3.0.34", pr_number:                                            11702, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:    11, deletions_count:   11},
		{sha: "2be2e6d0cafec7b9a4c5e01ace279772d1491392", date: "2022-03-08 06:15:06 UTC", description: "bump clap from 3.1.5 to 3.1.6", pr_number:                                                       11710, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      5, insertions_count:    10, deletions_count:   10},
		{sha: "f9bfa31fa72aa19e2f9cc02a2ad7c751d11b9e96", date: "2022-03-08 06:15:36 UTC", description: "bump async-graphql-warp from 3.0.31 to 3.0.33", pr_number:                                       11691, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "1a3abf6f0e941b94e5f1d9858d882460fa90479f", date: "2022-03-08 16:56:08 UTC", description: "Add vector user to systemd-journal-remote group", pr_number:                                     11713, scopes: ["releasing"], type:                              "enhancement", breaking_change: false, author: "Priit Laes", files_count:           1, insertions_count:    8, deletions_count:    3},
		{sha: "f8ee23dd3be680bbefda38d36ec1c5c87c7c04d6", date: "2022-03-08 07:56:34 UTC", description: "Remove the disabled http-to-http-noack soak", pr_number:                                         11717, scopes: [], type:                                         "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   5, insertions_count:    2, deletions_count:    130},
		{sha: "f4309402f2e207918644617154960b14c9b8642e", date: "2022-03-08 08:07:47 UTC", description: "Clippy allow output where expected", pr_number:                                                  11716, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        2, insertions_count:    6, deletions_count:    1},
		{sha: "fbdcea6879b570f206417776984eec4be5560e79", date: "2022-03-08 16:11:20 UTC", description: "bump anyhow from 1.0.55 to 1.0.56", pr_number:                                                   11715, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "9a1b92ab4b488fd0b2d8cf690e3bcba6c2c9e7d4", date: "2022-03-08 23:00:48 UTC", description: "Improve error codes for send failures", pr_number:                                               11719, scopes: ["redis sink"], type:                             "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:        1, insertions_count:    5, deletions_count:    5},
		{sha: "b6edb0203f684f67f8934da948cdf2bdd78d5236", date: "2022-03-09 03:02:02 UTC", description: "Replace PathIter lookup code", pr_number:                                                        11611, scopes: ["core"], type:                                   "chore", breaking_change:       false, author: "Nathan Fox", files_count:           89, insertions_count:   1083, deletions_count: 901},
		{sha: "752fc47e08acff340d8719b389ac04f267407201", date: "2022-03-09 00:29:28 UTC", description: "Try out nextest in CI", pr_number:                                                               11692, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        11, insertions_count:   25, deletions_count:   17},
		{sha: "17cd2772a887f26a16d2cf9c7b1d29c8b2955f6d", date: "2022-03-09 09:39:31 UTC", description: "bump regex from 1.5.4 to 1.5.5", pr_number:                                                      11725, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      5, insertions_count:    6, deletions_count:    6},
		{sha: "bd9c1ba8735789dba9ac63e7609397c90b9469ab", date: "2022-03-09 04:22:54 UTC", description: "Add debug statements to soak comment workflow", pr_number:                                       11730, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    32, deletions_count:   0},
		{sha: "9a94401c2f555f736297635341df4003c07ea659", date: "2022-03-09 13:51:47 UTC", description: "Expose `sinks::util::encoding::Transformer`", pr_number:                                         11724, scopes: [], type:                                         "chore", breaking_change:       false, author: "Pablo Sichert", files_count:        2, insertions_count:    4, deletions_count:    1},
		{sha: "9c8cb6d212798c1813f5119aa217572f553b755a", date: "2022-03-09 05:32:12 UTC", description: "Fix lookup of issue number in soak comment posting", pr_number:                                  11735, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    1, deletions_count:    1},
		{sha: "5402bb3aa74fdfa00a9b951daf3fe107b24b2317", date: "2022-03-09 09:09:16 UTC", description: "Remove redundant soak comment tasks", pr_number:                                                 11736, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    0, deletions_count:    14},
		{sha: "d06ee2ae2102a3e68a775a19a4ed34a6fab861db", date: "2022-03-09 12:10:23 UTC", description: "NATS sink+source authentication and TLS support", pr_number:                                     10688, scopes: ["auth"], type:                                   "enhancement", breaking_change: false, author: "seeyarh", files_count:              20, insertions_count:   1510, deletions_count: 118},
		{sha: "6448a291afacdb8a58f23492e348a4565dc71b2b", date: "2022-03-10 01:32:03 UTC", description: "make LimitedSender/LimitedReceiver wake up correctly", pr_number:                                11741, scopes: ["buffers"], type:                                "fix", breaking_change:         false, author: "Toby Lawrence", files_count:        1, insertions_count:    20, deletions_count:   17},
		{sha: "1b95e222e35175bc85d07bed909e0808f0e99fde", date: "2022-03-10 09:42:46 UTC", description: "refactor to emit events by batch", pr_number:                                                    11605, scopes: ["loki"], type:                                   "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:       2, insertions_count:    81, deletions_count:   25},
		{sha: "d86dd84fd25c245a5a926b1849e69ff11faaa876", date: "2022-03-10 04:11:20 UTC", description: "fix regression from path lookup changes", pr_number:                                             11746, scopes: ["aws_ec2_metadata transform"], type:             "fix", breaking_change:         false, author: "Nathan Fox", files_count:           3, insertions_count:    202, deletions_count:  101},
		{sha: "c6dc6c22a1d9ade38e9d6c087d2e7d67b1bdf2ef", date: "2022-03-10 17:26:18 UTC", description: "New redis source", pr_number:                                                                    7096, scopes: ["sources"], type:                                 "feat", breaking_change:        false, author: "舍我其谁", files_count:                 8, insertions_count:    707, deletions_count:  4},
		{sha: "6f2ac9f8b3223c1152cd58220e6f434016995f24", date: "2022-03-10 02:27:45 UTC", description: "bump tracing-core from 0.1.22 to 0.1.23", pr_number:                                             11740, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:    15, deletions_count:   15},
		{sha: "4167f52b72d2346007359c446c6e861b8f4f439c", date: "2022-03-10 12:00:54 UTC", description: "bump tracing from 0.1.31 to 0.1.32", pr_number:                                                  11750, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      8, insertions_count:    45, deletions_count:   45},
		{sha: "7e95f65d825e7f552f85c876501e642100722d88", date: "2022-03-10 05:10:36 UTC", description: "Fix soak comment debug logging", pr_number:                                                      11755, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    4, deletions_count:    4},
		{sha: "33bc3f15b05ce65d641a2dfda2d9c9d0ea1a1844", date: "2022-03-10 08:09:15 UTC", description: "rework `Fanout`", pr_number:                                                                     11348, scopes: ["performance"], type:                            "chore", breaking_change:       false, author: "Luke Steensen", files_count:        8, insertions_count:    428, deletions_count:  430},
		{sha: "d80d05d24c691cdf467db559607d710e9e1dafa5", date: "2022-03-10 16:38:17 UTC", description: "Add accept option to out_of_order_action", pr_number:                                            11133, scopes: ["loki sink"], type:                              "fix", breaking_change:         false, author: "Filip Pytloun", files_count:        6, insertions_count:    43, deletions_count:   66},
		{sha: "39f3b1829aa85b42bc334dd55aaecef4c005556b", date: "2022-03-10 18:07:20 UTC", description: "Unify logic to (de)serialize `OwnedPath`s", pr_number:                                           11759, scopes: [], type:                                         "chore", breaking_change:       false, author: "Pablo Sichert", files_count:        10, insertions_count:   153, deletions_count:  203},
		{sha: "ca84575230ad3a1d66e99e2133d0cf99716d2543", date: "2022-03-10 12:50:52 UTC", description: "Upload test results to Datadog", pr_number:                                                      11739, scopes: ["ci"], type:                                     "feat", breaking_change:        false, author: "Spencer Gilbert", files_count:      5, insertions_count:    53, deletions_count:   0},
		{sha: "476aa0511932d68f25c8c2bbaf7e043274c6067d", date: "2022-03-11 00:18:49 UTC", description: "migrate to new sink style", pr_number:                                                           11644, scopes: ["azure_blob sink"], type:                        "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:       6, insertions_count:    773, deletions_count:  746},
		{sha: "64e4f24a93e932e279a6b5232fb66cf2ff0ecb9b", date: "2022-03-11 09:11:14 UTC", description: "release/nightly: Drop EOL Debian 8 (Jessie) and add Debian 11", pr_number:                       11768, scopes: [], type:                                         "chore", breaking_change:       false, author: "Priit Laes", files_count:           2, insertions_count:    2, deletions_count:    2},
		{sha: "337ad8b187aa6b695040594ca1d57790f2beb8b4", date: "2022-03-11 03:01:27 UTC", description: "configuration schema RFC", pr_number:                                                            11634, scopes: ["config"], type:                                 "chore", breaking_change:       false, author: "Toby Lawrence", files_count:        1, insertions_count:    833, deletions_count:  0},
		{sha: "9a30a82fac62b23c80cc2623899e7c40e09153ce", date: "2022-03-11 08:11:15 UTC", description: "add fuzz testing for the VRL VM", pr_number:                                                     11024, scopes: ["vrl"], type:                                    "enhancement", breaking_change: false, author: "Stephen Wakely", files_count:       19, insertions_count:   2673, deletions_count: 34},
		{sha: "ddb98de99311ed2723ba9a2193b239eadfe87ad5", date: "2022-03-11 04:06:24 UTC", description: "Migrate to AWS SDK", pr_number:                                                                  11752, scopes: ["aws_cloudwatch_metrics sink"], type:            "chore", breaking_change:       false, author: "Nathan Fox", files_count:           8, insertions_count:    274, deletions_count:  169},
		{sha: "0d641c1d18303928e8ef758ce7ac2047b734187f", date: "2022-03-11 03:54:59 UTC", description: "Replace trivial `format!(\"{}\")` with `to_string`", pr_number:                                  11769, scopes: [], type:                                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        32, insertions_count:   53, deletions_count:   53},
		{sha: "ff436b2f22b3f26ff4c847113751b1f05445c84f", date: "2022-03-11 01:58:05 UTC", description: "bump arbitrary from 1.0.3 to 1.1.0", pr_number:                                                  11771, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "27e37e547af1bba2e4a64888e27ffc45135a15ff", date: "2022-03-11 02:47:51 UTC", description: "Further minimize the public interface", pr_number:                                               11764, scopes: [], type:                                         "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   4, insertions_count:    14, deletions_count:   23},
		{sha: "7fd146b0f02ac063c2c94c0f65166b412ce31329", date: "2022-03-11 02:52:59 UTC", description: "Introduce 'up' plots", pr_number:                                                                11775, scopes: [], type:                                         "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   3, insertions_count:    43, deletions_count:   1},
		{sha: "1e586f8c87e1ee9213ffb47ab88161876cb70f7d", date: "2022-03-11 06:43:45 UTC", description: "Allow test result upload to fail without failing the ci job", pr_number:                         11776, scopes: ["ci"], type:                                     "fix", breaking_change:         false, author: "Spencer Gilbert", files_count:      1, insertions_count:    3, deletions_count:    1},
		{sha: "027de98f8d35b68ba465b2506003b5e87860726f", date: "2022-03-11 08:29:15 UTC", description: "Update Loki soak to not reorder timestamps", pr_number:                                          11783, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    1, deletions_count:    0},
		{sha: "075315bb86e34719d873877c077880cabea04c47", date: "2022-03-11 08:33:59 UTC", description: "Utilize k8s static CPU manager policy for soaks", pr_number:                                     11782, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        2, insertions_count:    11, deletions_count:   5},
		{sha: "80a9eaed61ebd4cfd30cbab6890cd491f01b6b70", date: "2022-03-11 11:46:04 UTC", description: "Migrate to AWS SDK", pr_number:                                                                  11777, scopes: ["aws_cloudwatch_logs sink"], type:               "chore", breaking_change:       false, author: "Nathan Fox", files_count:           14, insertions_count:   540, deletions_count:  348},
		{sha: "e925b37e11eca7c395a94fdc589ad1a32678e7f9", date: "2022-03-11 11:04:51 UTC", description: "Use cargo-nextest for integration tests too", pr_number:                                         11726, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        29, insertions_count:   61, deletions_count:   34},
		{sha: "daf31ae17a5d9d0ca3bfdd77353038cb7c2cd9ee", date: "2022-03-12 02:07:24 UTC", description: "Try reading legacy encoding config first", pr_number:                                            11765, scopes: [], type:                                         "chore", breaking_change:       false, author: "Pablo Sichert", files_count:        1, insertions_count:    2, deletions_count:    2},
		{sha: "4a55953d607c658cba1ae6e90facdc9ab5b29660", date: "2022-03-12 02:13:41 UTC", description: "Integrate `encoding::Encoder` with `http` sink", pr_number:                                      11647, scopes: ["http sink", "codecs"], type:                    "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:        8, insertions_count:    214, deletions_count:  138},
		{sha: "2fcdff38ad0d2fb17578bb5d62851e09d006033a", date: "2022-03-12 06:21:57 UTC", description: "Transition local k8s dev to Tilt", pr_number:                                                    11804, scopes: ["dev"], type:                                    "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:      4, insertions_count:    76, deletions_count:   4},
		{sha: "b4e3b10cb3a55e4d82e72ef96a9468cf550acb5d", date: "2022-03-12 12:24:04 UTC", description: "add a since_now option", pr_number:                                                              11799, scopes: ["journald source"], type:                        "feat", breaking_change:        false, author: "Patrik", files_count:               2, insertions_count:    27, deletions_count:   2},
		{sha: "2159c895196705a2f9ebd56fe3e2d10c01db25ef", date: "2022-03-12 06:53:07 UTC", description: "abstract the I/O for disk v2 + add property test", pr_number:                                    11780, scopes: ["buffers"], type:                                "chore", breaking_change:       false, author: "Toby Lawrence", files_count:        21, insertions_count:   3310, deletions_count: 366},
		{sha: "574837ab63390ea9336aa78ffa9388fa2f30c348", date: "2022-03-12 07:55:17 UTC", description: "Migrate to AWS SDK", pr_number:                                                                  11781, scopes: ["aws_sqs sink"], type:                           "chore", breaking_change:       false, author: "Nathan Fox", files_count:           13, insertions_count:   172, deletions_count:  145},
		{sha: "5df28a437c94a668b92b7f7d63fc75d045582324", date: "2022-03-12 05:46:51 UTC", description: "Move upload of test results to CI workflow", pr_number:                                          11809, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        2, insertions_count:    43, deletions_count:   13},
		{sha: "ccdaa7f1a1087ab7b92f42393a8bd388eb7a026f", date: "2022-03-12 06:42:06 UTC", description: "Upload integration test JUnit reports to Datadog", pr_number:                                    11807, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    6, deletions_count:    0},
		{sha: "d82218fb29bbbd9234d8e8070c06baad160aabee", date: "2022-03-12 06:56:06 UTC", description: "bump pretty_assertions from 1.1.0 to 1.2.0", pr_number:                                          11798, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      5, insertions_count:    6, deletions_count:    6},
		{sha: "48ab7846b3a89776422f8db53f7f65a19d68920b", date: "2022-03-12 07:30:45 UTC", description: "bump metrics-exporter-prometheus from 0.7.0 to 0.9.0", pr_number:                                11792, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    55, deletions_count:   15},
		{sha: "015efd4204b52e3176c055bf43a7fd9bf5f813b6", date: "2022-03-14 21:02:28 UTC", description: "Ignore writer_waits_when_buffer_is_full for now", pr_number:                                     11815, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    1, deletions_count:    0},
		{sha: "8c489504f5c98ff47864c8922d2a976fc091cc05", date: "2022-03-14 21:39:26 UTC", description: "Fix docs", pr_number:                                                                            11824, scopes: ["redis source"], type:                           "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        2, insertions_count:    20, deletions_count:   2},
		{sha: "4a8b5876fb4e0fa63d6f5604f633dc71f5c323d4", date: "2022-03-15 00:30:46 UTC", description: "bump async-graphql from 3.0.34 to 3.0.35", pr_number:                                            11818, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:    11, deletions_count:   11},
		{sha: "75002242b2e2083454772300a2d79fb1621b1de2", date: "2022-03-15 04:21:34 UTC", description: "clean up duplicate code for setting tower layers", pr_number:                                    11828, scopes: ["sinks"], type:                                  "chore", breaking_change:       false, author: "Toby Lawrence", files_count:        38, insertions_count:   267, deletions_count:  235},
		{sha: "d768414adbe0b29a079c7f5bcdc400fd6e7d14a7", date: "2022-03-15 01:57:13 UTC", description: "Rebuild integration test docker images", pr_number:                                              11827, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    3, deletions_count:    0},
		{sha: "78cd3da8d792e9afaa26fba444bb6f657e9d0f81", date: "2022-03-15 02:57:13 UTC", description: "Wrap the maxminddb reader in Arc", pr_number:                                                    11831, scopes: ["geoip transform"], type:                        "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    4, deletions_count:    16},
		{sha: "de299f4c3eda1d586bde9ece2bc96f5569969c1e", date: "2022-03-15 03:49:32 UTC", description: "Move Linux tests to GHA custom runner", pr_number:                                               11813, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        2, insertions_count:    30, deletions_count:   118},
		{sha: "3397283e9b23fed0ee9e4be244fa949aef73a8dc", date: "2022-03-15 07:02:20 UTC", description: "Restore http-to-http-noack soak", pr_number:                                                     11826, scopes: [], type:                                         "chore", breaking_change:       false, author: "Will", files_count:                 4, insertions_count:    128, deletions_count:  0},
		{sha: "be41a28f2ebed9c25dc2ec5ee1cd5a289201947a", date: "2022-03-16 05:05:59 UTC", description: "datadog agent style secret management RFC", pr_number:                                           11536, scopes: ["config"], type:                                 "chore", breaking_change:       false, author: "Pierre Rognant", files_count:       1, insertions_count:    198, deletions_count:  0},
		{sha: "32106891d1eeda497d851a9a6a89813655e371cd", date: "2022-03-15 21:34:42 UTC", description: "Check if binaries are already installed", pr_number:                                             11846, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    9, deletions_count:    3},
		{sha: "88ff9a7727f01801100ec47501f83e2d1fc92f26", date: "2022-03-15 21:59:12 UTC", description: "bump async-stream from 0.3.2 to 0.3.3", pr_number:                                               11819, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:    6, deletions_count:    6},
		{sha: "b9c0b9581b0b4db6022192c0cadf721426b2570d", date: "2022-03-16 07:48:11 UTC", description: "bump docker/build-push-action from 2.9.0 to 2.10.0", pr_number:                                  11852, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:    5, deletions_count:    5},
		{sha: "f192531bb3764d57bfc80fbbe2b05d7eda8e8934", date: "2022-03-16 12:38:49 UTC", description: "Make `Remap` transform generic over `Runner`", pr_number:                                        11836, scopes: [], type:                                         "chore", breaking_change:       false, author: "Pablo Sichert", files_count:        2, insertions_count:    131, deletions_count:  54},
		{sha: "e3c7b2810350dac52690b5f82f827f60946a7462", date: "2022-03-16 05:55:38 UTC", description: "Ensure tasks are instrumented with active spans", pr_number:                                     11856, scopes: ["observability"], type:                          "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        19, insertions_count:   36, deletions_count:   31},
		{sha: "10ffefbb7e8731cc78e387439b06d3166391558f", date: "2022-03-16 14:06:09 UTC", description: "bump async-graphql-warp from 3.0.34 to 3.0.35", pr_number:                                       11838, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "218e76c7d0f9f8d68f0084cf77f61cc1ac1c9f30", date: "2022-03-16 14:09:23 UTC", description: "bump reqwest from 0.11.9 to 0.11.10", pr_number:                                                 11839, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:    16, deletions_count:   16},
		{sha: "a247e58efbd0391f94d87e2a9939939c17242d1f", date: "2022-03-16 14:15:07 UTC", description: "bump rkyv from 0.7.35 to 0.7.36", pr_number:                                                     11841, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    5, deletions_count:    5},
		{sha: "4723166610ba935f6e7f396cfd5e48c100776296", date: "2022-03-16 07:20:11 UTC", description: "bump dashmap from 5.1.0 to 5.2.0", pr_number:                                                    11842, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    6, deletions_count:    6},
		{sha: "1483bc1ac88e984346af5f2f2c1e5a501c968079", date: "2022-03-16 15:18:49 UTC", description: "bump libc from 0.2.119 to 0.2.120", pr_number:                                                   11843, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "4ac90abf2a3d6af745e1287cbd66918a74be1eb5", date: "2022-03-16 15:23:50 UTC", description: "bump nom from 7.1.0 to 7.1.1", pr_number:                                                        11840, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:    14, deletions_count:   15},
		{sha: "f8d1e9b4a2c8f7859eefd07588578e81dff8f2cf", date: "2022-03-16 09:45:26 UTC", description: "Require buffer sizes to be non-zero", pr_number:                                                 11829, scopes: ["buffers"], type:                                "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        18, insertions_count:   148, deletions_count:  80},
		{sha: "f5c6ea5f669af127c0ac20ace2ac3aa126880dca", date: "2022-03-16 23:33:05 UTC", description: "ignore comments in nested code", pr_number:                                                      11855, scopes: ["vrl"], type:                                    "fix", breaking_change:         false, author: "Stephen Wakely", files_count:       1, insertions_count:    32, deletions_count:   0},
		{sha: "fbd7700187f54fb924feb77b3410fed8a63c210a", date: "2022-03-17 04:32:35 UTC", description: "migration to stream sink", pr_number:                                                            11708, scopes: ["aws_sqs sink"], type:                           "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:       9, insertions_count:    610, deletions_count:  534},
		{sha: "1a972f2c046ceafc5fea19c6a427f03e21a10e6c", date: "2022-03-16 21:52:31 UTC", description: "move release builds over to custom hosted runners too", pr_number:                               11866, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        2, insertions_count:    13, deletions_count:   13},
		{sha: "b39b682e51fe580c8ed0ef97bf854fd773cd59db", date: "2022-03-16 23:29:51 UTC", description: "bump dyn-clone from 1.0.4 to 1.0.5", pr_number:                                                  11862, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      7, insertions_count:    8, deletions_count:    8},
		{sha: "a07dab3569ad40e1fac7e07b6d7ae68ec8107e1d", date: "2022-03-16 23:30:10 UTC", description: "bump crossbeam-queue from 0.3.4 to 0.3.5", pr_number:                                            11859, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    4, deletions_count:    4},
		{sha: "b15b620a6ebd047fdef557f770d47b8b07839163", date: "2022-03-16 23:30:33 UTC", description: "bump crossterm from 0.23.0 to 0.23.1", pr_number:                                                11860, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    4, deletions_count:    4},
		{sha: "e115e813c6a94e01471c8cf8cec9a83219b22f09", date: "2022-03-17 07:04:55 UTC", description: "Added VM highlight.", pr_number:                                                                 11845, scopes: ["external docs"], type:                          "docs", breaking_change:        false, author: "Stephen Wakely", files_count:       1, insertions_count:    122, deletions_count:  0},
		{sha: "f6e121bfabb834b9ccc2eaf1ed67f9c9e87e6e1f", date: "2022-03-17 04:02:36 UTC", description: "bump crossbeam-utils from 0.8.7 to 0.8.8", pr_number:                                            11861, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "5d52f3b399cb34763df778d005530959a294bc32", date: "2022-03-17 05:40:47 UTC", description: "Escape quotes in tag values", pr_number:                                                         11858, scopes: ["prometheus_exporter sink"], type:               "fix", breaking_change:         false, author: "Bruce Guenter", files_count:        1, insertions_count:    48, deletions_count:   4},
		{sha: "2029fa6f05ff0da5a8418e8e48dc42db36e573b6", date: "2022-03-18 05:05:36 UTC", description: "update VRL stdlib functions to work with the VM", pr_number:                                     11722, scopes: ["vrl"], type:                                    "enhancement", breaking_change: false, author: "Stephen Wakely", files_count:       139, insertions_count:  3324, deletions_count: 1693},
		{sha: "e118d2802fec3947c2fb4e218b5799a516afdff5", date: "2022-03-18 00:51:48 UTC", description: "move baseline timings workflows to GHA hosted runners", pr_number:                               11882, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    5, deletions_count:    5},
		{sha: "0103e64e4d813ad8fb66e67f2056b6d10930f68b", date: "2022-03-18 11:28:51 UTC", description: "update check-events script", pr_number:                                                          11747, scopes: ["observability"], type:                          "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:       38, insertions_count:   323, deletions_count:  179},
		{sha: "2c96a08a4ba8e9cbdfb0670f3607f8762739e739", date: "2022-03-18 04:20:43 UTC", description: "Fix debian package names", pr_number:                                                            11887, scopes: ["releasing"], type:                              "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        3, insertions_count:    47, deletions_count:   49},
		{sha: "6dc45d39b2f7a157a414e6639ba652db9e725ca1", date: "2022-03-18 08:55:28 UTC", description: "Fix debian packaging", pr_number:                                                                11889, scopes: ["releasing"], type:                              "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        3, insertions_count:    8, deletions_count:    5},
		{sha: "fb67fc91b3dffd52748d95b124fdb3eb54a00e2c", date: "2022-03-19 06:25:34 UTC", description: "remove batch settings from the sink config", pr_number:                                          11879, scopes: ["datadog_archives sink"], type:                  "fix", breaking_change:         false, author: "Vladimir Zhuk", files_count:        1, insertions_count:    6, deletions_count:    16},
		{sha: "f14333d58ef3d9cd0c0ee1af8eb291a3cf822a42", date: "2022-03-18 23:02:54 UTC", description: "Fix nightly deb verify", pr_number:                                                              11902, scopes: ["releasing"], type:                              "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        1, insertions_count:    2, deletions_count:    2},
		{sha: "4b72e889a62b125f222e9427372664a1c5c90287", date: "2022-03-19 02:39:44 UTC", description: "Migrate to AWS SDK", pr_number:                                                                  11868, scopes: ["aws_s3 sink"], type:                            "chore", breaking_change:       false, author: "Nathan Fox", files_count:           13, insertions_count:   324, deletions_count:  171},
		{sha: "c684f408eadc3ff7a3a7e86c90b2378cad28c3bd", date: "2022-03-19 07:52:36 UTC", description: "read message key as bytes instead of string", pr_number:                                         11903, scopes: ["kafka source"], type:                           "fix", breaking_change:         false, author: "Samuel Gabel", files_count:         1, insertions_count:    1, deletions_count:    1},
		{sha: "d569517d7e0cd580b06aea0cbddf384c4d9683f8", date: "2022-03-19 05:56:38 UTC", description: "Migrate to AWS SDK", pr_number:                                                                  11881, scopes: ["aws_kinesis_streams sink"], type:               "chore", breaking_change:       false, author: "Nathan Fox", files_count:           7, insertions_count:    198, deletions_count:  105},
		{sha: "9b06111bba7b47b965de486f8ae09f1942e41afe", date: "2022-03-19 10:06:12 UTC", description: "add VM option for conditions", pr_number:                                                        11801, scopes: ["vrl"], type:                                    "enhancement", breaking_change: false, author: "Stephen Wakely", files_count:       5, insertions_count:    115, deletions_count:  10},
		{sha: "1b6b7550252aec8b6208f81b5deac8cc336595b9", date: "2022-03-19 10:44:11 UTC", description: "fix remaining VM bugs", pr_number:                                                               11890, scopes: ["vrl"], type:                                    "fix", breaking_change:         false, author: "Stephen Wakely", files_count:       10, insertions_count:   101, deletions_count:  53},
		{sha: "27381ec978e00553746372679c1692682328d4ec", date: "2022-03-19 05:00:10 UTC", description: "Allow disabling reporting", pr_number:                                                           11912, scopes: ["blackhole sink"], type:                         "feat", breaking_change:        false, author: "Jesse Szwedko", files_count:        2, insertions_count:    22, deletions_count:   19},
		{sha: "3488abb4922429951da8db7887f2c9ae5c27a83d", date: "2022-03-19 13:08:01 UTC", description: "Fix naming for VRL runtime benches", pr_number:                                                  11892, scopes: [], type:                                         "chore", breaking_change:       false, author: "Pablo Sichert", files_count:        2, insertions_count:    2, deletions_count:    2},
		{sha: "3d9cc90905754641a24d0a46414d13eeb401bb6a", date: "2022-03-19 06:57:12 UTC", description: "Upgrade AWS SDK", pr_number:                                                                     11904, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        2, insertions_count:    48, deletions_count:   47},
		{sha: "1e2c8ae9655032f3595cef5d0cc56c4f35ec27f9", date: "2022-03-19 09:18:58 UTC", description: "Fix handling of acknowledgements", pr_number:                                                    11911, scopes: ["aws_sqs source"], type:                         "fix", breaking_change:         false, author: "Bruce Guenter", files_count:        1, insertions_count:    4, deletions_count:    5},
		{sha: "993bf88fb573af14efadf2f917919412ea7f3c1d", date: "2022-03-21 20:58:39 UTC", description: "Make check-msrv CI job name less opaque", pr_number:                                             11914, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    1, deletions_count:    1},
		{sha: "a42bb088e19ab8c094fc0d24b373e9daa4a2825f", date: "2022-03-21 21:43:38 UTC", description: "bump utf8-width from 0.1.5 to 0.1.6", pr_number:                                                 11919, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    9, deletions_count:    9},
		{sha: "3ee9273674e70fbc10f8e2fb846bd0989e1086fc", date: "2022-03-22 05:47:41 UTC", description: "bump libc from 0.2.120 to 0.2.121", pr_number:                                                   11918, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "24cdf59429c059e5778715c6ec96ce27e9cb4d2c", date: "2022-03-21 23:18:15 UTC", description: "Fix parse_xml handling of single node children", pr_number:                                      11910, scopes: ["vrl"], type:                                    "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        1, insertions_count:    34, deletions_count:   25},
		{sha: "dbab92eb8de695bf2aedc3f57a82d72bbf0449bb", date: "2022-03-22 01:43:11 UTC", description: "Convert decoder to iterator", pr_number:                                                         11913, scopes: ["aws_sqs source"], type:                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        1, insertions_count:    103, deletions_count:  44},
		{sha: "7cb951daed57448b80463303a07184ed537d9c54", date: "2022-03-22 04:19:52 UTC", description: "Add `s around `print_interval_secs` value", pr_number:                                           11915, scopes: ["blackhole sink"], type:                         "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        1, insertions_count:    1, deletions_count:    1},
		{sha: "d9161058de0ed9918c323b877f74d9a32b2f17b4", date: "2022-03-23 00:34:10 UTC", description: "reduce cloning for events", pr_number:                                                           11800, scopes: ["observability"], type:                          "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:       240, insertions_count:  858, deletions_count:  1528},
		{sha: "6d50007c1a7909844dab4b529a94f1815a481b16", date: "2022-03-22 21:21:32 UTC", description: "bump test-case from 2.0.0 to 2.0.1", pr_number:                                                  11930, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "0c33d2b2301b12ecf6954df3d4a4e13f8e6ce1cf", date: "2022-03-23 01:30:47 UTC", description: "bump peter-evans/create-or-update-comment from 1 to 2", pr_number:                               11936, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    1, deletions_count:    1},
		{sha: "dd314474f339443124f3e21afc16e15b1f5ae845", date: "2022-03-23 10:55:14 UTC", description: "bump actions/cache from 2.1.7 to 3", pr_number:                                                  11924, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:    8, deletions_count:    8},
		{sha: "0575a3a2b12569de9e561b818760511df9611ea6", date: "2022-03-23 05:40:48 UTC", description: "Optimize event send loop", pr_number:                                                            11925, scopes: ["aws_sqs source"], type:                         "enhancement", breaking_change: false, author: "Bruce Guenter", files_count:        1, insertions_count:    22, deletions_count:   22},
		{sha: "816fa485ed7e602a5b93c77557aaad1e9fbfaf7c", date: "2022-03-23 13:00:30 UTC", description: "add _unmatched route", pr_number:                                                                11875, scopes: ["route transform"], type:                        "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:       8, insertions_count:    91, deletions_count:   43},
		{sha: "bec2b1a2e7b7ca3c523e6ddbbfa8679dd62db1cb", date: "2022-03-23 13:14:07 UTC", description: "add `ip_ntop()` and `ip_pton()` functions", pr_number:                                           11917, scopes: ["vrl"], type:                                    "enhancement", breaking_change: false, author: "Hugo Hromic", files_count:          8, insertions_count:    370, deletions_count:  0},
		{sha: "f4ca6f645b151a0f05cfc7f9222f6d129649330c", date: "2022-03-24 04:38:18 UTC", description: "Migrate to AWS SDK", pr_number:                                                                  11906, scopes: ["aws_kinesis_firehose sink"], type:              "chore", breaking_change:       false, author: "Nathan Fox", files_count:           8, insertions_count:    222, deletions_count:  144},
		{sha: "901a3f2d14ef3da1fb114b97513664193b3409b1", date: "2022-03-24 06:07:47 UTC", description: "bump hyper from 0.14.17 to 0.14.18", pr_number:                                                  11949, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "cb6c79b832a016564b081fd43ff926ed8030b863", date: "2022-03-24 15:08:02 UTC", description: "bump log from 0.4.14 to 0.4.16", pr_number:                                                      11948, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "b3c1977de849b212b7e8bab275bc94c548bf6075", date: "2022-03-24 17:16:38 UTC", description: "update component spec", pr_number:                                                               11957, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:       1, insertions_count:    2, deletions_count:    3},
		{sha: "1d398321a87dd53eadad68a2f29b4a8de850fc39", date: "2022-03-25 01:48:30 UTC", description: "better integration of tracing/tokio-console", pr_number:                                         11954, scopes: ["observability"], type:                          "chore", breaking_change:       false, author: "Toby Lawrence", files_count:        15, insertions_count:   147, deletions_count:  137},
		{sha: "aab8338312c5f0fe3dd89a048d59d1bb4ba77b0f", date: "2022-03-25 07:03:54 UTC", description: "document soak dependency with tabulate", pr_number:                                              11967, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jérémie Drouet", files_count:       1, insertions_count:    1, deletions_count:    0},
		{sha: "22bf3bc1744d08ab16fef272eadc6289d4453540", date: "2022-03-24 23:10:22 UTC", description: "Upgrade Rust to 1.59.0", pr_number:                                                              11923, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        20, insertions_count:   31, deletions_count:   28},
		{sha: "c5f28a8ca7cb4bd2b8bd741f04affe48e9b5f8fa", date: "2022-03-25 08:24:04 UTC", description: "add `is_empty` function", pr_number:                                                             9732, scopes: ["vrl"], type:                                     "feat", breaking_change:        false, author: "Jean Mertz", files_count:           5, insertions_count:    226, deletions_count:  0},
		{sha: "1c5929cd0cdbc69310b366826ff0e340ef2eec63", date: "2022-03-25 01:46:33 UTC", description: "bump enumflags2 from 0.7.3 to 0.7.4", pr_number:                                                 11966, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    5, deletions_count:    5},
		{sha: "aea69e02cf4992ec9b611eb3b2ebce3f6db1da37", date: "2022-03-25 01:46:59 UTC", description: "bump maxminddb from 0.21.0 to 0.22.0", pr_number:                                                11965, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    13, deletions_count:   3},
		{sha: "4c6677dce89e41c97453798b71f58b698e2dd1ef", date: "2022-03-25 02:29:34 UTC", description: "Revise VM highlight from feedback", pr_number:                                                   11963, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        1, insertions_count:    2, deletions_count:    4},
		{sha: "99624cc82cbfdac1c4eaebb43e2f47c57714c412", date: "2022-03-25 09:38:53 UTC", description: "bump async-graphql from 3.0.35 to 3.0.36", pr_number:                                            11947, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:    11, deletions_count:   11},
		{sha: "e2889d7c0660302996a9ecb9a05beca0f14d8d9c", date: "2022-03-25 03:34:19 UTC", description: "Update lading, remove minikube from the soaks", pr_number:                                       11891, scopes: [], type:                                         "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:   170, insertions_count:  541, deletions_count:  4417},
		{sha: "8accca0eebfb1b21485d57fc046ff9f93bcca08a", date: "2022-03-25 03:59:38 UTC", description: "bump minimist from 1.2.5 to 1.2.6 in /website", pr_number:                                       11975, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    3, deletions_count:    3},
		{sha: "c751556d6bb10a56c184ee88263576e2e671503a", date: "2022-03-25 08:29:22 UTC", description: "Alias `/services/collector` endpoint", pr_number:                                                11941, scopes: ["splunk_hec source"], type:                      "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:        1, insertions_count:    24, deletions_count:   1},
		{sha: "d7d449d5efc3f23b4c54de16972ee8130b11f548", date: "2022-03-25 23:10:56 UTC", description: "add tls options for datadog metrics", pr_number:                                                 11955, scopes: ["datadog_metrics sink"], type:                   "chore", breaking_change:       false, author: "Nathan Fox", files_count:           2, insertions_count:    14, deletions_count:   2},
		{sha: "e94ee8e86903478fc311d070e27811c7c38622e7", date: "2022-03-26 03:56:53 UTC", description: "update docs for template missing fields", pr_number:                                             11933, scopes: ["external docs"], type:                          "docs", breaking_change:        false, author: "Stephen Wakely", files_count:       1, insertions_count:    2, deletions_count:    1},
		{sha: "f662074d41ed1d69d5f639361f1b89dfefbcd315", date: "2022-03-26 00:12:47 UTC", description: "Update error metrics in vector top", pr_number:                                                  11973, scopes: ["api"], type:                                    "fix", breaking_change:         false, author: "Will", files_count:                 5, insertions_count:    60, deletions_count:   27},
		{sha: "8a553b6ee62d6155ad6bdbd243ddbe9212123fca", date: "2022-03-25 22:56:15 UTC", description: "Downgrade to 1.58.1", pr_number:                                                                 11974, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        9, insertions_count:    8, deletions_count:    10},
		{sha: "23cef4a560053457bd8b487cd90adabc9ecfa26a", date: "2022-03-26 03:08:16 UTC", description: "Add account-id field", pr_number:                                                                11943, scopes: ["aws_ec2_metadata transform"], type:             "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:        2, insertions_count:    29, deletions_count:   0},
		{sha: "e80080e3a6809a20db6194391f827fc941afe4fe", date: "2022-03-26 04:46:48 UTC", description: "Count discarded events in received events", pr_number:                                           11962, scopes: ["buffers"], type:                                "fix", breaking_change:         true, author:  "Jesse Szwedko", files_count:        3, insertions_count:    19, deletions_count:   3},
		{sha: "e7761244409103831a84513ad47545beaa19a675", date: "2022-03-28 20:06:23 UTC", description: "bump semver from 1.0.6 to 1.0.7", pr_number:                                                     11998, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    5, deletions_count:    5},
		{sha: "2ed83753a0bf023518acb503a01651206776772c", date: "2022-03-28 20:06:46 UTC", description: "bump test-case from 2.0.1 to 2.0.2", pr_number:                                                  12000, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "e5fd262b81e0c27709e04bbfef1d6b1572242429", date: "2022-03-28 20:07:07 UTC", description: "bump rust_decimal from 1.22.0 to 1.23.1", pr_number:                                             12001, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "233e6f8664e1bf6fa510149f8245601fe06b6f35", date: "2022-03-28 20:18:53 UTC", description: "Alias `processed_bytes_total`", pr_number:                                                       11990, scopes: ["observability"], type:                          "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        36, insertions_count:   21, deletions_count:   187},
		{sha: "2929274f13a02a64d20b5e9168bc91aed53ea75e", date: "2022-03-29 03:34:57 UTC", description: "bump paste from 1.0.6 to 1.0.7", pr_number:                                                      12002, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "dac35ef44f46e05443142972608dad7624cf73c1", date: "2022-03-28 23:41:44 UTC", description: "remove use of `Sink` trait from `Fanout` and buffer-adjacent channels", pr_number:               11988, scopes: ["buffers", "core"], type:                        "chore", breaking_change:       false, author: "Toby Lawrence", files_count:        52, insertions_count:   1906, deletions_count: 2246},
		{sha: "6d3f7cb417d41cf2821e3217889376e27e6271b7", date: "2022-03-28 23:09:22 UTC", description: "Fix batch send error handling", pr_number:                                                       11945, scopes: ["aws_sqs source"], type:                         "fix", breaking_change:         false, author: "Bruce Guenter", files_count:        1, insertions_count:    15, deletions_count:   14},
		{sha: "4169d2a4aa07fee2e20dd789527c007cffc4e310", date: "2022-03-29 00:36:35 UTC", description: "add native protobuf and json codecs", pr_number:                                                 11929, scopes: ["codecs"], type:                                 "feat", breaking_change:        false, author: "Luke Steensen", files_count:        2062, insertions_count: 2047, deletions_count: 11},
		{sha: "a476967466e9649a9283fc09ce122105f8f6e41d", date: "2022-03-29 03:09:37 UTC", description: "Migrate to AWS SDK", pr_number:                                                                  11939, scopes: ["elasticsearch sink"], type:                     "chore", breaking_change:       false, author: "Nathan Fox", files_count:           10, insertions_count:   146, deletions_count:  258},
		{sha: "9434fd24fb79e74a10e6702229388e803992b68d", date: "2022-03-29 21:39:14 UTC", description: "bump async-graphql-warp from 3.0.35 to 3.0.36", pr_number:                                       11978, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "875cbbcf094545748a0f5fa100576ae389230e32", date: "2022-03-30 00:55:43 UTC", description: "Migrate to AWS SDK", pr_number:                                                                  11853, scopes: ["aws_s3 source"], type:                          "chore", breaking_change:       false, author: "Nathan Fox", files_count:           9, insertions_count:    585, deletions_count:  464},
		{sha: "f122b6082f0654d785e8b5f229c27e73c541b45f", date: "2022-03-30 03:13:38 UTC", description: "bump async-trait from 0.1.52 to 0.1.53", pr_number:                                              11999, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "0cbd46b6b84b30858e667c158e2c1c066154d44a", date: "2022-03-30 23:54:44 UTC", description: "Move codecs to lib", pr_number:                                                                  12007, scopes: ["codecs"], type:                                 "chore", breaking_change:       false, author: "Pablo Sichert", files_count:        2125, insertions_count: 1068, deletions_count: 1055},
		{sha: "5a8024b902fbdc248c76c9d0450683e180c9c4ee", date: "2022-03-31 02:16:03 UTC", description: "Add additional config options", pr_number:                                                       11981, scopes: ["aws_sqs source"], type:                         "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:        3, insertions_count:    52, deletions_count:   11},
		{sha: "17a62970dd8f888c878255dd5510353e9096f165", date: "2022-03-31 04:19:54 UTC", description: "Use kube-rs for kubernetes integration", pr_number:                                              11714, scopes: ["kubernetes_logs source"], type:                 "feat", breaking_change:        false, author: "Spencer Gilbert", files_count:      37, insertions_count:   411, deletions_count:  5659},
		{sha: "4b97cca4f0be3cc2d2ad4a0f717db40924e0b677", date: "2022-03-31 06:21:13 UTC", description: "Document what the return value of `length` is", pr_number:                                       12026, scopes: ["vrl"], type:                                    "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        1, insertions_count:    8, deletions_count:    3},
		{sha: "dac460cd418607c6369c8fe2e14efa636aaf661e", date: "2022-04-01 05:58:02 UTC", description: "update syslog source", pr_number:                                                                12040, scopes: ["build"], type:                                  "fix", breaking_change:         false, author: "Jérémie Drouet", files_count:       2, insertions_count:    12, deletions_count:   13},
		{sha: "037eb1c7cdabb1aac564cdf379f4e0c11e870261", date: "2022-04-01 03:54:55 UTC", description: "Remove port from `host` for `udp`", pr_number:                                                   12031, scopes: ["socket source"], type:                          "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        2, insertions_count:    2, deletions_count:    2},
		{sha: "5fd823f9ee4807d076a1907f49003edb8160d7fb", date: "2022-04-01 09:21:38 UTC", description: "bump kube from 0.69.1 to 0.70.0", pr_number:                                                     12035, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    12, deletions_count:   12},
		{sha: "2232e2ade79fce80c13660e0848952e4a52ac7bd", date: "2022-04-01 10:10:10 UTC", description: "bump rkyv from 0.7.36 to 0.7.37", pr_number:                                                     12037, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    5, deletions_count:    5},
		{sha: "47c8fbadce936f5756dd4490102a53833c0797c3", date: "2022-04-01 06:35:00 UTC", description: "use basic auth credentials from proxy urls", pr_number:                                          12016, scopes: ["config"], type:                                 "enhancement", breaking_change: false, author: "Dave Grochowski", files_count:      3, insertions_count:    95, deletions_count:   17},
		{sha: "357a9457d83178d2db5cc3c41435d91bb867b1db", date: "2022-04-01 14:15:44 UTC", description: "update check-event script", pr_number:                                                           12004, scopes: ["observability"], type:                          "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:       26, insertions_count:   284, deletions_count:  109},
		{sha: "82a71935ba516cff25cb7298e96719e23d2f5cb9", date: "2022-04-01 13:31:11 UTC", description: "bump async-graphql from 3.0.36 to 3.0.37", pr_number:                                            12036, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      4, insertions_count:    11, deletions_count:   11},
		{sha: "3568d6a56c5edc2b0f53d94603b6c366241cf47e", date: "2022-04-01 11:24:02 UTC", description: "Audit tls settings", pr_number:                                                                  12046, scopes: [], type:                                         "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        50, insertions_count:   57, deletions_count:   107},
		{sha: "baf1afc481cf303738438f1cf4de2915b9ee9f98", date: "2022-04-01 23:22:41 UTC", description: "Remove Rusoto SDK", pr_number:                                                                   12012, scopes: ["core"], type:                                   "chore", breaking_change:       false, author: "Nathan Fox", files_count:           40, insertions_count:   190, deletions_count:  1392},
		{sha: "9752625bf7938ec51a87328d196a5ba7cba7965b", date: "2022-04-01 23:25:14 UTC", description: "Add strlen function", pr_number:                                                                 12030, scopes: ["vrl"], type:                                    "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:        6, insertions_count:    135, deletions_count:  1},
		{sha: "ec9b6794deee72440bb1a3b8ca6ecff4842016b9", date: "2022-04-01 23:25:28 UTC", description: "Remove VOLUME declarations from Dockerfiles", pr_number:                                         12047, scopes: ["docker platform"], type:                        "fix", breaking_change:         true, author:  "Jesse Szwedko", files_count:        7, insertions_count:    19, deletions_count:   6},
		{sha: "4f079d2251779e5b065f9d3a655e04baff6be899", date: "2022-04-01 22:24:16 UTC", description: "bump async-graphql-warp from 3.0.36 to 3.0.37", pr_number:                                       12050, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "db613833e4e700bb3a05e9afd1eed0311d525583", date: "2022-04-01 22:24:28 UTC", description: "bump clap from 3.1.6 to 3.1.7", pr_number:                                                       12051, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      5, insertions_count:    12, deletions_count:   12},
		{sha: "a7cbb52edd6dc95881148151c13a9a45baf198bd", date: "2022-04-02 06:32:33 UTC", description: "Report config to Datadog OP for enterprise", pr_number:                                          11937, scopes: ["observability"], type:                          "enhancement", breaking_change: false, author: "Lee Benson", files_count:           14, insertions_count:   562, deletions_count:  269},
		{sha: "0e20db7a444548ed134bfdeb06fadf206da566bb", date: "2022-04-02 05:41:54 UTC", description: "bump trust-dns-proto from 0.21.1 to 0.21.2", pr_number:                                          12034, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    4, deletions_count:    4},
		{sha: "b9ffca3affb927fdb75c8f384c659db0b50679fc", date: "2022-04-02 08:49:12 UTC", description: "reduce the number of useless components", pr_number:                                             11849, scopes: ["pipelines transform"], type:                    "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:       19, insertions_count:   517, deletions_count:  187},
		{sha: "8c7440c4be5a9f6f90beb3651cd24d0e0a4d2861", date: "2022-04-02 05:13:16 UTC", description: "Move Windows tests to GHA custom runner", pr_number:                                             11814, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        8, insertions_count:    51, deletions_count:   94},
		{sha: "6117d90f7c37873d8fa24a8b8a92590f0e92a1a2", date: "2022-04-02 23:54:11 UTC", description: "improve VRL parser compilation times", pr_number:                                                12053, scopes: ["vrl"], type:                                    "chore", breaking_change:       false, author: "Jean Mertz", files_count:           5, insertions_count:    25, deletions_count:   79},
		{sha: "4a1dfeb0d085471ce1625ea87180e9e5b9955d09", date: "2022-04-05 07:26:09 UTC", description: "convert objects to Loki labels", pr_number:                                                      12041, scopes: ["loki sink"], type:                              "feat", breaking_change:        false, author: "Maksim Nabokikh", files_count:      2, insertions_count:    66, deletions_count:   15},
		{sha: "209649856b79b703931d0b09b0d04b854a25e952", date: "2022-04-05 05:35:59 UTC", description: "typoes", pr_number:                                                                              12058, scopes: [], type:                                         "docs", breaking_change:        false, author: "Tshepang Lekhonkhobe", files_count: 9, insertions_count:    11, deletions_count:   11},
		{sha: "d4efd89b1d16b8d24a409e1729d3a77c9c466f5c", date: "2022-04-05 02:21:39 UTC", description: "bump ansi-regex from 3.0.0 to 3.0.1 in /website", pr_number:                                     12056, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    3, deletions_count:    3},
		{sha: "a37e0c31d4c0a8f687b5bf5d3f2900a098d2a245", date: "2022-04-05 03:31:53 UTC", description: "update ansi-regex in website due to vulnerability", pr_number:                                   12070, scopes: [], type:                                         "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:      1, insertions_count:    3, deletions_count:    3},
		{sha: "20ab64df67aed79382e2f8f73b3eea0c0840a93c", date: "2022-04-05 01:36:40 UTC", description: "bump clap from 3.1.7 to 3.1.8", pr_number:                                                       12060, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      5, insertions_count:    10, deletions_count:   10},
		{sha: "7daf7d341e61932d5554214ec0adc9e85a1dd5c6", date: "2022-04-05 01:38:04 UTC", description: "bump pretty_assertions from 1.2.0 to 1.2.1", pr_number:                                          12063, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      5, insertions_count:    6, deletions_count:    6},
		{sha: "155d40fc2e229e70bf6f657a7d9388efd81c2541", date: "2022-04-05 01:38:21 UTC", description: "bump lru from 0.7.3 to 0.7.4", pr_number:                                                        12066, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "40120d3c520a9751642d401a60148d93622abd36", date: "2022-04-05 02:10:03 UTC", description: "bump tracing-core from 0.1.23 to 0.1.24", pr_number:                                             12065, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      3, insertions_count:    15, deletions_count:   15},
		{sha: "a9105b685f96dd62e1fe2116850112409ea9f61a", date: "2022-04-05 03:14:30 UTC", description: "Upgrade cue to version 0.4.2", pr_number:                                                        12072, scopes: [], type:                                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        1, insertions_count:    3, deletions_count:    3},
		{sha: "989d722379387849043ef3c7c333328356572a81", date: "2022-04-05 09:22:51 UTC", description: "bump EmbarkStudios/cargo-deny-action from 1.2.12 to 1.2.15", pr_number:                          12071, scopes: ["ci"], type:                                     "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "adc2cd0313c113d46dc466d9e7dd266316fe4a1f", date: "2022-04-05 11:24:15 UTC", description: "add support for custom request query parameters", pr_number:                                     12033, scopes: ["prometheus_scrape source"], type:               "enhancement", breaking_change: false, author: "Hugo Hromic", files_count:          2, insertions_count:    145, deletions_count:  0},
		{sha: "ba79d833c491d9a73ef9ed2682fd0740d65c67e5", date: "2022-04-05 11:03:25 UTC", description: "bump wiremock from 0.5.11 to 0.5.12", pr_number:                                                 12076, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "10ad00da043eaffdc32c8c5afd4b92d0040c689f", date: "2022-04-05 09:36:49 UTC", description: "Remove stray asterisk", pr_number:                                                               12079, scopes: ["external docs"], type:                          "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:      1, insertions_count:    1, deletions_count:    1},
		{sha: "26883b4d2af85690089913ad8130755cdf5f9900", date: "2022-04-05 14:03:36 UTC", description: "bump inherent from 1.0.0 to 1.0.1", pr_number:                                                   12081, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      1, insertions_count:    2, deletions_count:    2},
		{sha: "62c5900f9e3de0a1513de42ab47c66c97a6c519f", date: "2022-04-05 22:38:17 UTC", description: "Remove Helm from debian's list of supported installers", pr_number:                              12078, scopes: ["external docs"], type:                          "fix", breaking_change:         false, author: "Spencer Gilbert", files_count:      1, insertions_count:    1, deletions_count:    1},
		{sha: "d98411ab8d380cc2b1113dee1921c2faea6a7736", date: "2022-04-05 21:26:29 UTC", description: "Fix documentation of `query`", pr_number:                                                        12082, scopes: ["elasticsearch sink"], type:                     "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:        1, insertions_count:    12, deletions_count:   1},
		{sha: "aad72b9e3beb401eef49364890dfe6e4c5cce590", date: "2022-04-05 22:04:32 UTC", description: "bump lru from 0.7.4 to 0.7.5", pr_number:                                                        12077, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "3536fb956d045fd8d240f98fba290b65e0f8604a", date: "2022-04-06 07:11:53 UTC", description: "bump maxminddb from 0.22.0 to 0.23.0", pr_number:                                                12075, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      2, insertions_count:    3, deletions_count:    3},
		{sha: "0c1033a45ccbfc9bb54f90b0297f4198974c274f", date: "2022-04-06 08:38:42 UTC", description: "bump tracing-subscriber from 0.3.9 to 0.3.10", pr_number:                                        12061, scopes: ["deps"], type:                                   "chore", breaking_change:       false, author: "dependabot[bot]", files_count:      6, insertions_count:    8, deletions_count:    8},
		{sha: "7bf62b8d0e9e7d66371367d9d08420f323aca88d", date: "2022-04-06 18:58:40 UTC", description: "compression support for vector v2", pr_number:                                                   12059, scopes: ["vector sink"], type:                            "feat", breaking_change:        false, author: "Mathew Heard", files_count:         6, insertions_count:    58, deletions_count:   6},
		{sha: "1aa99c8abd21e409ccd38bc662690a3f606f0343", date: "2022-04-06 05:53:16 UTC", description: "Ignore the HEC token for health checks", pr_number:                                              12098, scopes: ["splunk_hec source"], type:                      "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        1, insertions_count:    39, deletions_count:   17},
		{sha: "8ebdc63914f38f034b8153f39cae0d1437848b10", date: "2022-04-07 00:00:47 UTC", description: "Implement `BytesEncoder` framing", pr_number:                                                    12093, scopes: ["codecs"], type:                                 "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:        4, insertions_count:    84, deletions_count:   8},
		{sha: "9afb079e588706ae9d5d9711c527ae6d785873c1", date: "2022-04-07 03:01:04 UTC", description: "add local variable scope to blocks", pr_number:                                                  12017, scopes: ["vrl"], type:                                    "feat", breaking_change:        true, author:  "Jean Mertz", files_count:           166, insertions_count:  902, deletions_count:  646},
		{sha: "eaa43b1bfdd511ebe8cee82c31d9f67ab5076758", date: "2022-04-06 22:57:13 UTC", description: "RFC for improving S3's support for large uploads", pr_number:                                    11835, scopes: [], type:                                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:        1, insertions_count:    345, deletions_count:  0},
		{sha: "606a1a470cd71113b0a679758d1ebf58fdcc1bd1", date: "2022-04-06 23:44:54 UTC", description: "Refine error codes", pr_number:                                                                  12102, scopes: ["observability"], type:                          "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        16, insertions_count:   197, deletions_count:  62},
		{sha: "aa43a4cb321cd1391a95ec90e451adb091ceda14", date: "2022-04-07 09:16:43 UTC", description: "stop failing early for component features check", pr_number:                                     12104, scopes: ["ci"], type:                                     "enhancement", breaking_change: false, author: "Jérémie Drouet", files_count:       1, insertions_count:    1, deletions_count:    1},
		{sha: "7d13fc95f2364848f9fc040de4b6f8fe0f502b2a", date: "2022-04-07 02:00:14 UTC", description: "Remove extra error labels", pr_number:                                                           12106, scopes: ["observability"], type:                          "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        4, insertions_count:    5, deletions_count:    7},
		{sha: "6fe2010d6542323aaf3e6030266957d44e0da517", date: "2022-04-07 11:08:28 UTC", description: "Initial `datadog_traces` sink", pr_number:                                                       11489, scopes: ["new sink"], type:                               "feat", breaking_change:        false, author: "Pierre Rognant", files_count:       19, insertions_count:   1231, deletions_count: 2},
		{sha: "623b18263ff4281c6c42cb9635a5403eb3991747", date: "2022-04-07 02:36:52 UTC", description: "Allow batch timeout to be fractional seconds", pr_number:                                        11812, scopes: ["sinks"], type:                                  "feat", breaking_change:        false, author: "Jesse Szwedko", files_count:        60, insertions_count:   84, deletions_count:   128},
		{sha: "24abc61e4de326366488dd423f5827ce333a8fe9", date: "2022-04-07 05:56:53 UTC", description: "Batch default should be f64 for `datadog_traces`", pr_number:                                    12110, scopes: [], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:        2, insertions_count:    4, deletions_count:    5},
		{sha: "1a3d48024a98783b126018ac2a8df833f17c1cc0", date: "2022-04-08 07:28:04 UTC", description: "missing file for `datadog-traces` sink integrations test", pr_number:                            12117, scopes: ["ci"], type:                                     "fix", breaking_change:         false, author: "Pierre Rognant", files_count:       1, insertions_count:    32, deletions_count:   0},
		{sha: "82ea429a7fe42ce48c5d52dffb2801507c6ca81b", date: "2022-04-08 07:49:50 UTC", description: "early traces doc", pr_number:                                                                    11021, scopes: ["external docs"], type:                          "docs", breaking_change:        false, author: "Pierre Rognant", files_count:       82, insertions_count:   133, deletions_count:  0},
		{sha: "57c799b88b7ad05ba600d386cebb8af304fbf014", date: "2022-04-12 11:32:53 UTC", description: "fix error processing when loading configuration files", pr_number:                               12149, scopes: ["config"], type:                                 "fix", breaking_change:         false, author: "Hugo Hromic", files_count:          1, insertions_count:    1, deletions_count:    1},
		{sha: "8cbd4784d54ee6605579035595979bb8825f370e", date: "2022-04-13 06:57:57 UTC", description: "`buffer_events` should decrement discarded", pr_number:                                          12170, scopes: ["buffers"], type:                                "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:        3, insertions_count:    35, deletions_count:   13},
		{sha: "2724091a40b4733148719cb532805217ece5316f", date: "2022-04-13 20:45:45 UTC", description: "Re-enable concurrency", pr_number:                                                               11761, scopes: ["loki sink"], type:                              "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:        4, insertions_count:    63, deletions_count:   16},
		{sha: "4798837219b6bb69993846efba8600c154c6e58c", date: "2022-04-13 20:46:21 UTC", description: "Add ability to enrich with peer addr port", pr_number:                                           12032, scopes: ["socket source"], type:                          "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:        11, insertions_count:   84, deletions_count:   29},
	]
}
