package metadata

releases: "0.48.0": {
	date:     "2025-06-30"
	codename: ""

	whats_next: []

	description: """
		The Vector team is excited to announce version `0.48.0`!

		This release new configuration options for various components.
		For example, VRL expressions can be used in HTTP
		query parameters.

		Also, this release includes numerous bug fixes which should improve Vector's reliability in
		production environments.
		"""

	changelog: [
		{
			type: "enhancement"
			description: """
				The HTTP client source now supports VRL within query parameters.

				For example:
				```yaml
				sources:
					http:
						type: http_client
						endpoint: https://endpoint.com
						method: GET
						query:
							timestamp:
								 value: "now()"
								 type: "vrl"
							foo:
								 value: "bar"
								 type: "string"
				```
				
				This means that HTTP requests can now be made with dynamic query parameters.
				This is particularly useful for generating unique timestamps or UUIDs per request.
				"""
			contributors: ["benjamin-awd"]
		},
		{
			type: "feat"
			description: """
				The `kubernetes_logs` source now includes a new `max_merged_line_bytes` configuration option. This setting enables users to cap the size of log lines after they’ve been combined using `auto_partial_merge`. Previously, the `max_line_bytes` field only restricted line sizes *before* merging, leaving no practical way to limit the length of merged lines—unless you set a size so tiny that it prevented merging altogether by stopping short of the continuation character. This new option gives you better control over merged line sizes.
				"""
			contributors: ["ganelo"]
		},
		{
			type: "feat"
			description: """
				Adds trace data type support to the Axiom sink allowing propagation of
				OpenTelemetry traces received via the `opentelemetry` or compatible trace
				emitting components.
				"""
			contributors: ["darach"]
		},
		{
			type: "fix"
			description: """
				The `amqp` sink now attempts to re-connect to the AMQP broker when the channel has been disconnected. It will also create up to 4 channels in a pool (configurable with the `max_channels` setting) to improve throughput.
				"""
			contributors: ["aramperes"]
		},
		{
			type: "enhancement"
			description: """
				Updated the Splunk HEC source to accept requests that contain the header content-type with any value containing "application/json," not the exact value of "application/json." This matches the behavior of a true Splunk HEC. Allows sources from AWS to successfully send events to the Splunk HEC source without additional proxying to update headers.
				"""
			contributors: ["Tot19"]
		},
		{
			type: "fix"
			description: """
				The dnstap source now correctly labels DNS response code 16 as BADVERS instead of BADSIG, which is reserved for TSIG RRs.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "fix"
			description: """
				Improved `dnstap` source TCP backpressure and load handling.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "feat"
			description: """
				Enabled URL path access in VRL scripts of custom auth strategy for server components.
				"""
			contributors: ["byronwolfman"]
		},
		{
			type: "fix"
			description: """
				Unknown fields in the `tls` config are now rejected so these fields now need to be removed for Vector to start successfully.
				"""
			contributors: ["thomasqueirozb"]
		},
		{
			type: "feat"
			description: """
				Added `rate_limit_num` and `rate_limit_duration_secs` options to `kafka` sink, to enable rate limiting this sink.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "fix"
			description: """
				Fixed panic in `opentelemetry` source when a `NaN` float value is received. `NaN` values are now converted to `null`.
				"""
			contributors: ["srstrickland"]
		},
		{
			type: "feat"
			description: """
				VRL programs can now read, write, and delete the `interval_ms` field in metric events.
				"""
			contributors: ["thomasqueirozb"]
		},
		{
			type: "feat"
			description: """
				Added vector uptime in seconds to `vector top`.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "feat"
			description: """
				The `socket` source with `udp` mode now supports joining multicast groups via the `multicast_groups` option
				of that source. This allows the source to receive multicast packets from the specified multicast groups.
				
				Note that in order to work properly, the `socket` address must be set to `0.0.0.0` and not
				to `127.0.0.1` (localhost) or any other specific IP address. If other IP address is used, the host's interface
				will filter out the multicast packets as the packet target IP (multicast) would not match the host's interface IP.
				"""
			contributors: ["jorgehermo9"]
		},
		{
			type: "enhancement"
			description: """
				Adds support for session tokens in AWS authentication options. When using temporary credentials (access key, secret key, and session token), the session token is required. Temporary credentials can be provided by an external system and updated using the `SECRET` backend.
				"""
			contributors: ["anil-db"]
		},
		{
			type: "fix"
			description: """
				Fix panic when dnstap parser encounters unusual timestamps.
				"""
			contributors: ["wooffie"]
		},
		{
			type: "fix"
			description: """
				The `elasticsearch` sink now encodes parameters such as `index` that contain characters that need to
				be escaped in JSON strings.
				"""
			contributors: ["jszwedko"]
		},
		{
			type: "fix"
			description: """
				Fix crash when dnstap source parses unexpected socket address values.”
				"""
			contributors: ["wooffie"]
		},
		{
			type: "fix"
			description: """
				Fix bug allowing invalid Prometheus timestamps; now properly rejected during parsing.
				"""
			contributors: ["wooffie"]
		},
		{
			type: "fix"
			description: """
				The `aws_ecs_metrics` source now skips over empty ECS metrics payloads. It previously failed to parse such payloads.
				"""
			contributors: ["tustvold"]
		},
	]

	vrl_changelog: """
		### [0.25.0 (2025-06-26)]
		
		#### Enhancements
		
		- Add support for decompressing lz4 frame compressed data.
		
		authors: jimmystewpot (https://github.com/vectordotdev/vrl/pull/1367)
		"""

	commits: [
		{sha: "968f3c91c3db6c1b57f4a329cb03f1978744b18d", date: "2025-05-17 01:33:24 UTC", description: "relax prepare script branch creation", pr_number: 23067, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "e76f18ec820cf381716eca16c51ccdd11963af17", date: "2025-05-20 01:07:29 UTC", description: "Handling bad addresses in dsntap", pr_number: 23071, scopes: ["parsing"], type: "fix", breaking_change: false, author: "Burkov Egor", files_count: 2, insertions_count: 56, deletions_count: 1},
		{sha: "bf57582d5855cf7beae12eb88bf21e911680489c", date: "2025-05-20 00:03:23 UTC", description: "update VRL changelog block insertion", pr_number: 23075, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 91, deletions_count: 54},
		{sha: "eb12304ebd8b2b06fd50df242d660b056a33b8a3", date: "2025-05-20 19:51:36 UTC", description: "update git repo url for homebrew release", pr_number: 23081, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "0ce2de7527170e2b9792a7916391a8d5483bc949", date: "2025-05-21 00:03:02 UTC", description: "improve release templates", pr_number: 23080, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 73, deletions_count: 31},
		{sha: "34db5b0d784392e8d1c687edb6682ca980dcace8", date: "2025-05-21 01:20:19 UTC", description: "final release steps", pr_number: 23085, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 60, insertions_count: 465, deletions_count: 210},
		{sha: "bb6a91404cdc3f662b037cc4d5f52237b28d215d", date: "2025-05-22 01:31:12 UTC", description: "Refactor `struct SourceSender` a bit", pr_number: 23089, scopes: ["sources"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 58, deletions_count: 72},
		{sha: "3528fc19162a6c722209423ea1db0e2e754bfaf8", date: "2025-05-28 20:43:47 UTC", description: "Move `TransformOutputsBuf` functions out of test flag", pr_number: 23116, scopes: ["transforms"], type: "chore", breaking_change: false, author: "Josué AGBEKODO", files_count: 1, insertions_count: 0, deletions_count: 5},
		{sha: "5ac8263dc400033552cc86a1885a6f8a2d3cfd0a", date: "2025-06-05 05:20:48 UTC", description: "Add trace data support to Axiom Configuration for the axiom sink", pr_number: 22935, scopes: ["axiom sink"], type: "feat", breaking_change: false, author: "Darach Ennis", files_count: 2, insertions_count: 6, deletions_count: 1},
		{sha: "c6cc716d12cd3e16fa3504ae8353ee294f53c0b7", date: "2025-06-06 02:53:40 UTC", description: "Updates SMP cli client version", pr_number: 23125, scopes: ["ci"], type: "chore", breaking_change: false, author: "Scott Opell", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "2f8f1c3474f7f5a0717ab8f1879750844e07ff05", date: "2025-06-09 17:54:22 UTC", description: "Bump check-spelling/check-spelling from 0.0.24 to 0.0.25", pr_number: 23143, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "03e6215ee4bdb37790e2356ce89990d461079ada", date: "2025-06-09 17:54:49 UTC", description: "Bump aws-actions/configure-aws-credentials from 4.1.0 to 4.2.1", pr_number: 23142, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "a34ab883c1682444c55e5f15757e20c024f763db", date: "2025-06-09 17:55:33 UTC", description: "Bump ossf/scorecard-action from 2.4.1 to 2.4.2", pr_number: 23141, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "59adb1aa767d471d2de3c3ac705e8dfe35e3c5d9", date: "2025-06-09 21:55:58 UTC", description: "Bump docker/build-push-action from 6.16.0 to 6.18.0", pr_number: 23140, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "a04b4d3bd094902a2aa92feed981b21687e3fd68", date: "2025-06-09 22:01:18 UTC", description: "Bump tokio from 1.44.2 to 1.45.1", pr_number: 23136, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 11, insertions_count: 16, deletions_count: 16},
		{sha: "e5f533e86c6914369b3b34c9e68a0f23ec85e7d9", date: "2025-06-09 22:01:39 UTC", description: "Bump quickcheck_macros from 1.0.0 to 1.1.0", pr_number: 23135, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "563a9f048195f10e81044edca867d7788e755bd8", date: "2025-06-09 22:01:59 UTC", description: "Bump uuid from 1.16.0 to 1.17.0", pr_number: 23134, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 5},
		{sha: "ddb412160664cee505619e75f69a3d56fd9f337b", date: "2025-06-09 22:03:55 UTC", description: "Bump crc from 3.2.1 to 3.3.0", pr_number: 23132, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "fcc546a4ad7229f949852044900e6ff997016905", date: "2025-06-09 22:04:41 UTC", description: "Bump confy from 0.6.1 to 1.0.0", pr_number: 23130, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 28},
		{sha: "a03f45c511ef3893317153b07ba0fc2f51cf12ef", date: "2025-06-09 22:23:41 UTC", description: "Bump windows-service from 0.7.0 to 0.8.0", pr_number: 22557, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "2e067e7054dd4cd88f4b2416407b49b902ccb447", date: "2025-06-09 21:23:45 UTC", description: "Handle sources with no default output", pr_number: 23172, scopes: ["sources"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 13, deletions_count: 6},
		{sha: "4539b58964ab8bf23fb8f51a34718e233f7555db", date: "2025-06-10 04:47:04 UTC", description: "fix rcode 16 handling in dnsmsg-parser", pr_number: 23106, scopes: ["dnstap source"], type: "fix", breaking_change: false, author: "Ensar Sarajčić", files_count: 6, insertions_count: 10, deletions_count: 5},
		{sha: "1bf3328af6c4f4d01a9830a34a7078906431ecff", date: "2025-06-10 01:54:25 UTC", description: "add docker compose debug logs (if a test fails)", pr_number: 23176, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 84, deletions_count: 19},
		{sha: "6c1922e2769c49b70db179f94e9969802e319ef8", date: "2025-06-10 18:16:38 UTC", description: "update Rust to 1.86", pr_number: 23175, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 41, insertions_count: 68, deletions_count: 67},
		{sha: "ab886d8e1b1f22ffae76ebc999c9494971ec0780", date: "2025-06-10 19:56:11 UTC", description: "update cargo-deny version to support 2024 edition", pr_number: 23178, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "0191c3af087c1eb00094c7076452d27f19c92e50", date: "2025-06-11 02:44:51 UTC", description: "Bump ring from 0.17.12 to 0.17.14 in the cargo group", pr_number: 23020, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "f84dbbc915b525d4ba54257bbf40de209468b2f6", date: "2025-06-11 05:59:56 UTC", description: "Timestamp error handling in dnstap parser", pr_number: 23072, scopes: ["parsing"], type: "fix", breaking_change: false, author: "Burkov Egor", files_count: 2, insertions_count: 9, deletions_count: 5},
		{sha: "d1da30726228589ec8f84e38052feb60e67afdc7", date: "2025-06-10 23:18:08 UTC", description: "Bump the patches group across 1 directory with 27 updates", pr_number: 23179, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 11, insertions_count: 252, deletions_count: 231},
		{sha: "7082f2e5f0b809f8625ee5093247bd7ac5cadcec", date: "2025-06-11 00:22:29 UTC", description: "Fix spelling and add QA'd to allow.txt", pr_number: 23182, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 2, deletions_count: 1},
		{sha: "333cd765f0d8bd414413caac1b12b07960efece8", date: "2025-06-11 13:48:41 UTC", description: "Allow content-type header if it includes application/json", pr_number: 23024, scopes: ["splunk_hec source"], type: "feat", breaking_change: false, author: "tot19", files_count: 2, insertions_count: 97, deletions_count: 10},
		{sha: "c03abd71ec406e4ec49b14d3461cc800f52f3286", date: "2025-06-11 23:32:11 UTC", description: "multicast udp socket support", pr_number: 22099, scopes: ["sources"], type: "feat", breaking_change: false, author: "Jorge Hermo", files_count: 8, insertions_count: 289, deletions_count: 14},
		{sha: "25296d0d01c8e2e9eac87096b2bff5ea07094afe", date: "2025-06-12 00:52:11 UTC", description: "Prometheus timestamp parse int overflow handle", pr_number: 23077, scopes: ["parsing"], type: "fix", breaking_change: false, author: "Burkov Egor", files_count: 2, insertions_count: 32, deletions_count: 4},
		{sha: "60fdc8a1cf96963bc99d2b0148befb11a6d75f34", date: "2025-06-11 19:19:31 UTC", description: "Add deny_unknown_fields to TlsEnableableConfig", pr_number: 23187, scopes: ["core"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 4, deletions_count: 0},
		{sha: "ce5e6d445254b5f284076dd70c1b2ced27825c90", date: "2025-06-11 23:38:20 UTC", description: "remove vector-test-harness from REVIEWING.md", pr_number: 23192, scopes: ["internal docs"], type: "docs", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "5b8afb4d20c5678ff43e9eb91eaf7dbbb425c231", date: "2025-06-12 04:53:22 UTC", description: "Bump tempfile from 3.19.1 to 3.20.0", pr_number: 23137, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 11, insertions_count: 15, deletions_count: 15},
		{sha: "bf9f3444833aa8334125632bc0ba92dcaebb47d2", date: "2025-06-12 17:34:33 UTC", description: "Bump snafu from 0.7.5 to 0.8.0", pr_number: 19481, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 12, deletions_count: 12},
		{sha: "064efc13e37ae69500c2ae8c4cd5bb168605dc0f", date: "2025-06-12 18:20:02 UTC", description: "Add enableable to allow.txt", pr_number: 23197, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "dffcb5a02c881dd94a84470722ca4e6040a9cc43", date: "2025-06-13 02:29:08 UTC", description: "Enable `internal_log_rate_limit` by default", pr_number: 22899, scopes: ["dev"], type: "feat", breaking_change: false, author: "Shivanth MP", files_count: 78, insertions_count: 135, deletions_count: 176},
		{sha: "fbb2dea4c1fedf514d7e6fa0d764c45e489ee3a4", date: "2025-06-13 04:59:59 UTC", description: "add rate limit configuration for `kafka` sink", pr_number: 23196, scopes: ["kafka sink"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 5, insertions_count: 59, deletions_count: 2},
		{sha: "865c8aca2007e4e280dffae27babc33fee079216", date: "2025-06-12 22:02:13 UTC", description: "allow to provide aws session token", pr_number: 22964, scopes: ["sinks"], type: "enhancement", breaking_change: false, author: "Anil Gupta", files_count: 24, insertions_count: 189, deletions_count: 1},
		{sha: "f31839be8b0b7425fdc3c3fc44e6557dece627ee", date: "2025-06-13 01:11:44 UTC", description: "Allow specification of a maximum line size to be applied after merging instead of just before", pr_number: 22582, scopes: ["kubernetes_logs source"], type: "enhancement", breaking_change: false, author: "Orri Ganel", files_count: 14, insertions_count: 358, deletions_count: 54},
		{sha: "7205b4a7273ec4480d6e03375df01b29002df504", date: "2025-06-13 17:55:42 UTC", description: "attempt one reconnect when channel has errored", pr_number: 22971, scopes: ["amqp sink"], type: "fix", breaking_change: false, author: "Aram Peres", files_count: 12, insertions_count: 163, deletions_count: 38},
		{sha: "e6ac12207dbe53c2ebe638d94199175c65fa9f65", date: "2025-06-13 20:58:12 UTC", description: "Bump rustls from 0.20.9 to 0.22.4", pr_number: 21808, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 9, deletions_count: 9},
		{sha: "89f73419b94c4ac82e4e9dc9f422e87fa41445d9", date: "2025-06-13 18:33:41 UTC", description: "Handle NaN in opentelemetry source without panic", pr_number: 23201, scopes: ["opentelemetry source"], type: "fix", breaking_change: false, author: "Scott Strickland", files_count: 2, insertions_count: 39, deletions_count: 1},
		{sha: "521cad69a33ad25016f1acad15492d50186fdd49", date: "2025-06-14 11:06:27 UTC", description: "Fix config extension typo in Windows command (from-source.md)", pr_number: 23166, scopes: ["website"], type: "docs", breaking_change: false, author: "Tenghuan He", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "e74575a27e255e7202c2fd05481c23104165bb6a", date: "2025-06-13 20:10:09 UTC", description: "Encode bulk action parameters as JSON", pr_number: 21293, scopes: ["elasticsearch sink"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 122, deletions_count: 40},
		{sha: "ed67c6d6c41ef87b43dee6a80d5f808c8684ae7d", date: "2025-06-16 22:52:43 UTC", description: "skip over empty ECS metrics payloads", pr_number: 23151, scopes: ["aws_ecs_metrics source"], type: "fix", breaking_change: false, author: "Raphael Taylor-Davies", files_count: 2, insertions_count: 24, deletions_count: 4},
		{sha: "932bb6797db4766e66714bf18c6529cf99c7366a", date: "2025-06-16 23:53:16 UTC", description: "Bump pulsar to 6.3.1", pr_number: 22862, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jakub Onderka", files_count: 3, insertions_count: 74, deletions_count: 55},
		{sha: "71be227f29febf1efc82859ef19677f99ec82ac4", date: "2025-06-16 21:17:53 UTC", description: "add missing perms to gardener_remove_waiting_author.yml", pr_number: 23213, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 6, deletions_count: 1},
		{sha: "f41cae298cd16d3360d0239c39b48ee076e434a9", date: "2025-06-16 21:34:57 UTC", description: "render input and output badges for all components", pr_number: 23204, scopes: ["website"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 11, insertions_count: 234, deletions_count: 179},
		{sha: "42ab24cc74f0b0a8fb3fa95c0b829ade1d226c78", date: "2025-06-16 21:37:59 UTC", description: "Remove docker_logs.txt and fix annotations", pr_number: 23214, scopes: ["internal"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 0, deletions_count: 2},
		{sha: "b8fdf424cb7d4bdc1b987c7fc24f37d4683c8b3b", date: "2025-06-16 21:56:13 UTC", description: "add Docker gitignore rules", pr_number: 23215, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 16, deletions_count: 1},
		{sha: "3ff55b87bc4362a24d9b06b91b5fc8efb1246875", date: "2025-06-16 22:11:47 UTC", description: "fix memory enrichment table highlight wording", pr_number: 23216, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 4, deletions_count: 13},
		{sha: "ae13b3b4d26a82a5f2585362f1b8c5d8368f8d47", date: "2025-06-17 03:53:04 UTC", description: "fix opentelemetry typo", pr_number: 22586, scopes: ["website"], type: "docs", breaking_change: false, author: "Matt Simons", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "e594c7b575030a4c51dc108a412f89a2d91ea259", date: "2025-06-17 05:34:56 UTC", description: "Bump greptimedb", pr_number: 23210, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jakub Onderka", files_count: 2, insertions_count: 9, deletions_count: 22},
		{sha: "fc1146aaaf89d4def75ee43854e7f31a76a290ae", date: "2025-06-17 00:40:03 UTC", description: "prettier rc formatting of hugo/html", pr_number: 23218, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 1889, deletions_count: 1789},
		{sha: "d720b303d2e87b75767a07a92a6fd3b52a180e01", date: "2025-06-17 18:22:04 UTC", description: "Make interval_ms modifiable via VRL", pr_number: 23217, scopes: ["core"], type: "feat", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 95, deletions_count: 43},
		{sha: "11e1cea3135bc22500fe90c9d9ac884892bf6da8", date: "2025-06-17 16:56:25 UTC", description: "Fix `InlineSingleUseReferencesVisitor` failing merges", pr_number: 23207, scopes: ["config"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 137, deletions_count: 54},
		{sha: "05d1521fc0fb08e74104a86b52e5241deda57d88", date: "2025-06-18 09:08:47 UTC", description: "allow VRL in query parameters", pr_number: 22706, scopes: ["http_client source"], type: "enhancement", breaking_change: false, author: "Benjamin Dornel", files_count: 12, insertions_count: 627, deletions_count: 61},
		{sha: "abd5e0ca26c3328a7c3e7acc1aa5f283033703d6", date: "2025-06-18 00:11:25 UTC", description: "add input telemetry types section for transforms and sinks", pr_number: 23222, scopes: ["website"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 35, insertions_count: 256, deletions_count: 69},
		{sha: "2f71bb716fc052493e5aa396456cf9db1dd92722", date: "2025-06-17 23:27:07 UTC", description: "use latest VRL and update function signature", pr_number: 23221, scopes: ["core"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 73, deletions_count: 68},
		{sha: "4cca9474763f234333d9c88d5f1a8efe76b0717b", date: "2025-06-18 00:26:08 UTC", description: "minor improvements to output data section", pr_number: 23225, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "154fe3ef90919bcbbbb4b7f093be67fa28228533", date: "2025-06-19 00:07:21 UTC", description: "fix vector exiting if nats sink url fails dns resolution or is unavailable without --require-healthy", pr_number: 23167, scopes: ["nats sink"], type: "fix", breaking_change: false, author: "rdwr-tomers", files_count: 3, insertions_count: 20, deletions_count: 7},
		{sha: "aded15be572117b2dd5c7fc1874a18fb84ec2572", date: "2025-06-19 02:44:32 UTC", description: "Fixed documentation badges space on documentation", pr_number: 23229, scopes: ["website"], type: "fix", breaking_change: false, author: "melinoix", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "8488649f616ddb410cac09a00ac8c38fb1a9c275", date: "2025-06-19 04:25:32 UTC", description: "implement all TCP dnstap options to reduce error", pr_number: 23123, scopes: ["dnstap source"], type: "fix", breaking_change: false, author: "Ensar Sarajčić", files_count: 2, insertions_count: 137, deletions_count: 36},
		{sha: "7f20e0f3b4ec35f09b55edc830b7633d684f8a74", date: "2025-06-20 19:53:09 UTC", description: "sort allow.txt lines", pr_number: 23237, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 274, deletions_count: 274},
		{sha: "ebfa262363eceed03c08671253dd93534138e80f", date: "2025-06-20 21:34:09 UTC", description: "improve gitignore", pr_number: 23239, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 41, deletions_count: 18},
		{sha: "cd8d07fc2d25268b68a0eed5cdc01d78bcd3e304", date: "2025-06-20 20:47:11 UTC", description: "improve environment variable docs", pr_number: 23236, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 105, deletions_count: 42},
		{sha: "675245880fbf3582a17c567e153d733db9b6a695", date: "2025-06-20 21:22:19 UTC", description: "improve and remove authorization requirements", pr_number: 23224, scopes: ["ci"], type: "feat", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 29, deletions_count: 43},
		{sha: "0e8f813f6b1e6dacbb927740f622e127a0933a2f", date: "2025-06-20 21:30:32 UTC", description: "remove obsolete and unused team.cue", pr_number: 23238, scopes: ["internal"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 162},
		{sha: "207a608fbda792fad0b271134d4af7b9bc731712", date: "2025-06-20 22:06:33 UTC", description: "Remove Page source and Edit this page buttons", pr_number: 23240, scopes: ["website"], type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 0, deletions_count: 28},
		{sha: "01bbc690b04253071fce3a6bab513ff71049b9b7", date: "2025-06-21 04:08:04 UTC", description: "add uptime seconds display to `vector top`", pr_number: 23228, scopes: ["observability"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 7, insertions_count: 53, deletions_count: 12},
		{sha: "ba2b75b8b44855106ad15fa4f790b8410755dcf7", date: "2025-06-21 03:38:30 UTC", description: "Bump convert_case from 0.7.1 to 0.8.0", pr_number: 22556, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 11, deletions_count: 2},
		{sha: "a0ef7aaed4368cdc99eca177524fc77669a0f6e2", date: "2025-06-23 17:31:31 UTC", description: "fix changelog check by fetching correct ref", pr_number: 23241, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 3},
		{sha: "96f8624e225deaee9bb1a20a6e2bba7774972332", date: "2025-06-24 05:53:35 UTC", description: "Update tower to v`0.5`", pr_number: 23186, scopes: ["deps"], type: "chore", breaking_change: false, author: "Serendo", files_count: 8, insertions_count: 87, deletions_count: 68},
		{sha: "6a88922da9b4112be6f0ab835cf039391db25327", date: "2025-06-23 21:11:08 UTC", description: "improve decoder docs", pr_number: 23245, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 23, insertions_count: 195, deletions_count: 45},
		{sha: "2bb9e8ef43f949c23e59549bbfd7ed2e63667003", date: "2025-06-23 21:11:24 UTC", description: "update PULL_REQUEST_TEMPLATE", pr_number: 23248, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 32, deletions_count: 33},
		{sha: "2fda2e7cf116251016a29cd3da874a860235fc3d", date: "2025-06-23 21:32:09 UTC", description: "add author to changelog", pr_number: 23250, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "94c964a3f27a9afab479fe42e9930db4a0ca6a2d", date: "2025-06-23 21:58:03 UTC", description: "add/fix telemetry input & output for transforms", pr_number: 23247, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 13, insertions_count: 98, deletions_count: 0},
		{sha: "1ee4d2f025b14553083211820b51ff74755612d3", date: "2025-06-24 00:23:52 UTC", description: "fix telemetry types for Sinks", pr_number: 23251, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 9, insertions_count: 65, deletions_count: 22},
		{sha: "212320097b49e39e7bce93619ffa9095a590b84d", date: "2025-06-24 01:17:30 UTC", description: "always run 'integration-tests' for the MQ", pr_number: 23252, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 2},
		{sha: "55a8371376fc4da71a56bc35be438a483889c823", date: "2025-06-24 18:00:18 UTC", description: "minor fixes for minor release tmpl", pr_number: 23258, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 2},
		{sha: "d9dc37c1867db1e07431770b2b1b4cb1953267b5", date: "2025-06-24 18:27:02 UTC", description: "re-add nats to integration tests and revert broken PR", pr_number: 23256, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 5, insertions_count: 9, deletions_count: 23},
		{sha: "820e63b77803bb0995b9aed10ecefc15547ca69c", date: "2025-06-24 19:10:05 UTC", description: "add access to URL path in custom VRL auth", pr_number: 23165, scopes: ["sources"], type: "feat", breaking_change: false, author: "Byron Wolfman", files_count: 5, insertions_count: 51, deletions_count: 13},
		{sha: "93f82b7ccae468b6f813588c4503ec9933dc615f", date: "2025-06-24 20:35:51 UTC", description: "avoid IT setup if suite will not run", pr_number: 23259, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 7, deletions_count: 4},
		{sha: "b364a1e692894df8843707e8f26c67a21e9497ce", date: "2025-06-25 17:18:36 UTC", description: "don't run K8S E2E tests on website only changes", pr_number: 23260, scopes: ["ci"], type: "feat", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 9, deletions_count: 2},
		{sha: "9d3d0a87187890f1efe5ce03cd7f067fd75d64b0", date: "2025-06-25 20:37:42 UTC", description: "add `int build` command", pr_number: 23265, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 10, insertions_count: 166, deletions_count: 98},
		{sha: "e4bfd06469d405ddb561c110b158fb1cc85aaf37", date: "2025-06-25 21:25:37 UTC", description: "refactoring (preparation for follow up PRs)", pr_number: 23266, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 17, deletions_count: 12},
		{sha: "516bbc91b21559ed99f835428d456fac0af391cc", date: "2025-06-25 22:07:35 UTC", description: "fix telemetry types for Sources", pr_number: 23267, scopes: ["website"], type: "chore", breaking_change: false, author: "Thomas", files_count: 16, insertions_count: 470, deletions_count: 349},
		{sha: "150208ab6af00f80e0d5a71a8b4f48e106c51cba", date: "2025-06-26 00:48:47 UTC", description: "K8S E2E tests skipping changes.yml in MQ", pr_number: 23268, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 9, deletions_count: 8},
		{sha: "022c840ebe1c3ef6e506bd9d9c1b596c7624f610", date: "2025-06-26 17:19:22 UTC", description: "remove unusued all-tests placeholder", pr_number: 23269, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 8, deletions_count: 22},
		{sha: "1357e1a3ab03e044e5aad33210898229a823dc20", date: "2025-06-26 19:53:18 UTC", description: "use git2 for some git operations", pr_number: 23270, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 83, deletions_count: 8},
	]
}
