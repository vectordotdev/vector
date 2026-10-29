package metadata

releases: "0.61.0": {
	date: "2026-10-29"

	changelog: [
		{
			type: "feat"
			description: #"""
				Added an `opensearch` sink for self-managed OpenSearch, Amazon OpenSearch Service,
				and Amazon OpenSearch Serverless. It shares the Elasticsearch sink's configuration
				options and delivery behavior. The new sink selects an OpenSearch-compatible bulk
				API version by default, so an explicit `api_version: v7` is not needed.
				"""#
			pr_numbers: [26722]
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				The `aws_ecs_metrics` source now uses the correct values for `ECS_CONTAINER_METADATA_URI_V4` and `ECS_CONTAINER_METADATA_URI` to resolve the metadata endpoint, ensuring metadata v4 endpoints are targeted properly.
				"""#
			pr_numbers: [26729]
			contributors: ["Captain1653"]
		},
		{
			type: "feat"
			description: #"""
				The `datadog_agent` source now accepts the v3 series metrics intake format used by default by Datadog Agent 7.81.0 and newer.
				"""#
			pr_numbers: [26264]
			contributors: ["arfa79"]
		},
		{
			type: "fix"
			description: #"""
				Fixed `vdev check docs` reporting unrelated dirty working tree files after running `cue fmt`. The check now snapshots CUE files before and after formatting so only files actually modified by the formatter are reported.
				"""#
			pr_numbers: [26725]
			contributors: ["patrickswedish"]
		},
		{
			type: "enhancement"
			description: #"""
				The VRL playground loads faster and keeps controls disabled until ready.
				"""#
			pr_numbers: [26813]
			contributors: ["pront"]
		},
		{
			type: "feat"
			description: #"""
				VRL now includes a `range` function that creates an array of integers from a start value to an exclusive end value, with an optional step.
				"""#
			pr_numbers: [26876]
			contributors: ["pront"]
		},
		{
			type: "enhancement"
			description: #"""
				The `character_delimited` and `newline_delimited` decoding framers now report frames dropped for
				exceeding `max_length` (`oversized_action: drop`) as an error: the message is logged at `error`
				level and `component_errors_total` is incremented with `error_code="decoder_frame_too_long"`.
				Previously these drops were only logged as a warning. Dropping behavior is otherwise unchanged.
				"""#
			pr_numbers: [26746]
			contributors: ["taylorchandleryoung"]
		},
		{
			type: "security"
			description: #"""
				The `databricks_zerobus` sink now verifies server certificates and hostnames for OAuth and Unity Catalog schema requests. Previously, these requests disabled TLS verification, which could expose OAuth credentials and schema responses to a network attacker.
				"""#
			pr_numbers: [26925]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: #"""
				Vector now reports disk buffer lifecycle events during startup and configuration reloads. Each event is logged and increments the `topology_disk_buffer_lifecycle_events_total` counter with an `event` tag:
				
				- `removed`: A removed sink still had data in its disk buffer, or Vector could not read how much data it held. The log says whether the sink's current instance keeps draining the buffer in the background or whether the reload waits for it to finish.
				- `replacement_unreferenced`: A changed sink no longer uses its previous disk buffer. If the reload succeeds, the new sink will not read any data left in the old buffer.
				- `unreferenced`: A disk buffer in the data directory is not used by the current configuration. It may still be draining through a detached sink instance. If not, any data it holds is not being processed.
				"""#
			pr_numbers: [26050]
			contributors: ["Jansen-w"]
		},
		{
			type: "security"
			description: #"""
				The `distroless-libc` Docker image now uses `gcr.io/distroless/cc-debian13` as its base instead of `cc-debian12`. The Debian 12 CA bundle still trusts the `emSign ECC Root CA - G3` and `AAA Certificate Services` roots that the eMudhra intermediate certificate compromise affects (CVE-2026-111943). The Debian 13 bundle removes them.
				"""#
			pr_numbers: [26911]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				The `gcp_pubsub` source now reconnects immediately, without logging an error, when the server
				resets a streaming pull. Since Vector 0.35.0 it logged an error and waited `retry_delay_secs`.
				It also trusts every certificate in `tls.ca_file`, not only the last one.
				"""#
			pr_numbers: [26770]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				The `gcp_pubsub` source no longer delays Vector shutdown when the shutdown starts while the source
				connects, waits to retry, or holds an open stream with acknowledgements enabled.
				"""#
			pr_numbers: [26815]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				The `gcp_pubsub` source now honors `tls.verify_certificate`, `tls.verify_hostname`,
				`tls.server_name`, and `tls.alpn_protocols`. It also uses the configured proxy.
				"""#
			pr_numbers: [26808]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				The `gcp_pubsub` source and HTTPS connections from `vector top` and `vector tap`
				now use the same system certificate authorities as other Vector components.
				"""#
			pr_numbers: [26808]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				The `vector` and `opentelemetry` sources now answer gRPC requests for unregistered services with
				gRPC status `UNIMPLEMENTED` instead of HTTP 404. These requests no longer log an error or
				increment `component_errors_total`.
				"""#
			pr_numbers: [26770]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				The `mqtt` sink now closes its broker connection when the sink stops, for example when a configuration reload removes or replaces it. Before, the connection stayed open, and the broker could repeatedly disconnect a replacement sink that used the same client ID.
				
				Before it stops, the sink sends its queued messages and completes pending QoS exchanges. A configuration reload that waits for the sink, for example because the replacement reuses its buffer, also waits for this step. If the connection fails during this step, the sink stops without a reconnect and the reload continues.
				"""#
			pr_numbers: [26793]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: #"""
				Avoid storing a copy of the default schema definition on every event when the shared default applies, reducing allocation and reference-count overhead when creating events.
				"""#
			pr_numbers: [26744]
			contributors: ["bruceg"]
		},
		{
			type: "enhancement"
			description: #"""
				Optimized the log event estimated size cache to eliminate locked operations on
				its value while building events, improving ingestion performance.
				"""#
			pr_numbers: [26713]
			contributors: ["bruceg"]
		},
		{
			type: "fix"
			description: #"""
				Fixed a race in the `redis` source with `data_type: list` that could lose a list entry during graceful shutdown. The source now waits up to five seconds for an in-flight pop to finish and forwards any returned entry before stopping. Blocking pops time out after one second on an empty list, keeping shutdown responsive when Redis is healthy.
				"""#
			pr_numbers: [26691]
			contributors: ["dsavov"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "Scheme-less `vector` sink endpoints default to `https`"
			anchor:   "vector-sink-default-https"
			description: #"""
				`address` and `routing.endpoints` values in the `vector` sink that omit the scheme (for example,
				`127.0.0.1:6000` instead of `http://127.0.0.1:6000`) now default to `https`. Previously they
				defaulted to `http` unless TLS was enabled. The `http` default was
				[deprecated in 0.59.0](https://vector.dev/deprecations/).
				
				Endpoints that already specify a scheme, and scheme-less endpoints in configurations that enable
				TLS, are unaffected. A scheme-less endpoint pointing at a plaintext `vector` source now fails the
				TLS handshake.
				"""#
			pr_numbers: [26699]
			contributors: ["JinchengCao"]
		},
		{
			type: "fix"
			description: #"""
				The `vector` sink now sends configured credentials to HTTP forward proxies and applies
				`tls.server_name` to TLS connections through CONNECT proxies.
				"""#
			pr_numbers: [26770]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Fixed a startup race in `vector top` on Unix. Depending on thread timing, it could exit with
				`The cursor position could not be read within a normal duration` before drawing the dashboard.
				"""#
			pr_numbers: [26817]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "security"
			description: #"""
				Updated the bundled Mozilla root certificate store to remove the `emSign ECC Root CA - G3` root following the eMudhra intermediate certificate compromise, along with other roots Mozilla has since distrusted. TLS connections to servers whose chains end at a removed root now fail validation.
				"""#
			pr_numbers: [26748]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: #"""
				Configuration reloads on Windows no longer wait an extra 200 milliseconds after the changed components stop.
				"""#
			pr_numbers: [26832]
			contributors: ["thomasqueirozb"]
		},
	]

	vrl_changelog: #"""
		### [0.38.0 (2026-10-28)](https://github.com/vectordotdev/vrl/releases/tag/v0.38.0)
		
		#### Security
		
		- Updated `rustls-webpki` to 0.103.16 to address [RUSTSEC-2026-0382](https://rustsec.org/advisories/RUSTSEC-2026-0382). TLS certificate verification in `http_request` now rejects certificate chains that contain the compromised eMudhra emSign intermediate certificates, even when the system trust store still trusts their root.
		
		*Thanks to [thomasqueirozb](https://github.com/thomasqueirozb) for contributing PR [#2008](https://github.com/vectordotdev/vrl/pull/2008)!*
		
		#### New Features
		
		- Added a new `range` function that generates an array of integers from `start` (inclusive) to `end` (exclusive), with an optional `step`.
		
		*Thanks to [lchabert](https://github.com/lchabert) for contributing PR [#1994](https://github.com/vectordotdev/vrl/pull/1994)!*
		
		#### Enhancements
		
		- `http_request` now uses the platform certificate verifier for HTTPS connections.
		
		*Thanks to [pront](https://github.com/pront) for contributing PR [#2026](https://github.com/vectordotdev/vrl/pull/2026)!*
		- Accept `critical` keyword in `to_syslog_severity` in addition to `crit`.
		
		*Thanks to [slopggobblercobbler81](https://github.com/slopggobblercobbler81) for contributing PR [#2010](https://github.com/vectordotdev/vrl/pull/2010)!*
		- VRL programs that assign objects or arrays now run faster and allocate less: an assignment whose value
		isn't used (for example, any statement that isn't the last expression of a block) no longer makes a deep
		copy of the assigned value.
		
		*Thanks to [lukesteensen](https://github.com/lukesteensen) for contributing PR [#2002](https://github.com/vectordotdev/vrl/pull/2002)!*
		
		#### Fixes
		
		- `to_unix_timestamp` with `unit: "nanoseconds"` is now correctly marked as fallible by the
		compiler. The conversion can fail at runtime when the timestamp is out of range for a
		64-bit nanosecond value, but the compiler previously rejected error handling for this
		call as unnecessary.
		
		Programs that call `to_unix_timestamp` with `unit: "nanoseconds"` are affected: they no
		longer compile until the error is handled. Other units are unaffected.
		
		**Before**
		
		```vrl
		to_unix_timestamp(ts, unit: "nanoseconds")
		```
		
		**After**
		
		```vrl
		to_unix_timestamp!(ts, unit: "nanoseconds")
		```
		
		or with error coalescing:
		
		```vrl
		to_unix_timestamp(ts, unit: "nanoseconds") ?? 0
		```
		
		*Thanks to [suryakant-source](https://github.com/suryakant-source) for contributing PR [#2001](https://github.com/vectordotdev/vrl/pull/2001)!*
		
		"""#
}
