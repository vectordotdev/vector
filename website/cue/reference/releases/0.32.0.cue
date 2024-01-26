package metadata

releases: "0.32.0": {
	date:     "2023-08-15"
	codename: ""

	description: """
		The Vector team is pleased to announce version 0.32.0!

		Be sure to check out the [upgrade guide](/highlights/2023-08-15-0-32-0-upgrade-guide) for
		breaking changes in this release.

		In addition to the usual enhancements and bug fixes, this release includes:

		- a new `greptimedb` sink for sending metrics to
		    [GreptimeDB](https://github.com/greptimeteam/greptimedb)
		- a new `protobuf` codec that can be used on sources that support codecs to decode incoming
		  protobuf data
		"""

	known_issues: [
		"""
			A number of sinks emit incorrect telemetry for the `component_sent_*` metrics:

			- WebHDFS
			- GCP Cloud Storage
			- AWS S3
			- Azure Blob Storage
			- Azure Monitor Logs
			- Databend
			- Clickhouse
			- Datadog Logs

			This is fixed in v0.32.1.
			""",
		"""
			The newly added `--openssl-legacy-provider` flag cannot actually be disabled by setting
			it to `false` via `--openssl-legacy-provider=false`. Instead it complains of extra
			arguments. This is fixed in v0.32.1.
			""",
		"""
			For AWS components, using `assume_role` for authentication without an `external_id`
			caused a panic. This is fixed in v0.32.2`.
			""",
	]

	changelog: [
		{
			type: "fix"
			scopes: ["lua transform"]
			description: """
				The `lua` transform now sets the `source_id` metadata to its own component ID if an
				event is emitted by the transform that has no origin `source_id` (e.g. events
				constructed in the transform itself).
				"""
			pr_numbers: [17870]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				VRL conditions included in configurations (e.g. the `filter` transform) are now
				checked at boot-time to ensure that they return a boolean instead of treating all
				non-boolean return values as `false` .
				"""
			pr_numbers: [17894]
		},
		{
			type: "fix"
			scopes: ["vector sink"]
			description: """
				The `vector` sink now considers `DataLoss` responses to be hard errors at indicates
				the a sink in the downstream `vector` source rejected the data. The `vector` sink
				will now not retry these errors and also reject them in any connected sources (when
				`acknowledgements` are enabled).
				"""
			contributors: ["sbalmos"]
			pr_numbers: [17904]
		},
		{
			type: "fix"
			scopes: ["vector sink"]
			description: """
				The `vector` sink now correctly applies configured HTTPS proxy settings. Previously
				it would fail to validate the downstream certificate.
				"""
			contributors: ["joemiller"]
			pr_numbers: [17651]
		},
		{
			type: "feat"
			scopes: ["new sink", "greptimedb sink"]
			description: """
				A new `greptimedb` sink was added allowing Vector to send metrics to
				[GreptimeDB](https://github.com/greptimeteam/greptimedb).
				"""
			contributors: ["sunng87"]
			pr_numbers: [17198]
		},
		{
			type: "fix"
			scopes: ["splunk_hec source"]
			description: """
				The `splunk_hec` source now treats the fields on incoming events as "flat" rather
				than interpreting them as field paths. For example, an incoming `foo.bar` field is
				now inserted as `{"foo.bar": "..."}` rather than `{"foo": {"bar": "..."}}`. This
				avoids panics that were caused by invalid paths.
				"""
			pr_numbers: [17943]
		},
		{
			type: "feat"
			scopes: ["config"]
			description: """
				Configuration fields that are field lookups (such as `log_schema.timestamp_key`) are
				now parsed at boot-time rather than run-time. In addition to better performance,
				this also means that invalid paths return an error at start time rather than being
				silently ignored at runtime.
				"""
			pr_numbers: [17947, 18024, 18058, 18084, 18097, 18090, 18099, 18139, 18124, 18160, 18109, 18185, 18212]
		},
		{
			type: "enhancement"
			scopes: ["clickhouse sink"]
			description: """
				The `clickhouse` sink `database` and `table` options are now
				[templatable](/docs/reference/configuration/template-syntax/).
				"""
			pr_numbers: [18005, 17972]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				Fractional second configuration options are now correctly parsed as fractional.
				Previously they would round to the nearest second.
				"""
			contributors: ["sbalmos"]
			pr_numbers: [17917]
		},
		{
			type: "fix"
			scopes: ["reload"]
			description: """
				The Vector API can now correctly be disabled during reload by setting `api.enabled`
				to `false`.
				"""
			contributors: ["KH-Moogsoft"]
			pr_numbers: [17958]
		},
		{
			type: "fix"
			scopes: ["observability"]
			description: """
				The `component_received_event_bytes_total` and `component_sent_event_bytes_total`
				metrics for sinks are now calculated _after_ any `encoding.only_fields` or
				`encoding.except_fields` options are applied.
				"""
			pr_numbers: [17941]
		},
		{
			type: "enhancement"
			scopes: ["prometheus_scrape source"]
			description: """
				The `prometheus_scrape` source now scrapes configured targets in parallel.
				"""
			contributors: ["nullren"]
			pr_numbers: [18021]
		},
		{
			type: "enhancement"
			scopes: ["prometheus_scrape source"]
			description: """
				The `prometheus_scrape` source now has a `scrape_timeout_secs` option to configure
				how long Vector should wait for each request.
				"""
			contributors: ["nullren"]
			pr_numbers: [18021]
		},
		{
			type: "enhancement"
			scopes: ["releasing"]
			description: """
				Vector's `debian` Docker images are now based on Debian 12 (Bookworm).
				"""
			pr_numbers: [18057]
		},
		{
			type: "fix"
			scopes: ["websocket sink"]
			description: """
				The `websocket` sink now correctly sends data as binary for "binary" codecs: `raw`,
				`native`, and `avro`. Previously it would always interpret the bytes as text
				(UTF-8).
				"""
			contributors: ["zhongchen"]
			pr_numbers: [18060]
		},
		{
			type: "enhancement"
			scopes: ["codecs"]
			description: """
				Vector sources that support codecs now support `protobuf` as an option. A Protobuf
				descriptor file must also be provided to use to decode the data.
				"""
			contributors: ["Daniel599"]
			pr_numbers: [18057]
		},
		{
			type: "fix"
			scopes: ["syslog source"]
			description: """
				The `syslog` source now correctly handles escape sequences appearing the structured
				data segment.
				"""
			pr_numbers: [18114]
		},
		{
			type: "fix"
			scopes: ["config"]
			description: """
				Numeric compression levels can now be set when using TOML. Previously Vector would
				fail to parse the configuration. This already worked for YAML and JSON
				configurations.
				"""
			pr_numbers: [18173]
		},
		{
			type: "fix"
			scopes: ["sinks"]
			description: """
				Sinks that support Adaptive Request Concurrency options now support configuring an
				`initial_concurrency` to start the concurrency limit at rather than starting at
				a limit of `1`.
				"""
			contributors: ["blake-mealey"]
			pr_numbers: [18175]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			description: """
				VRL's `encode_logfmt` function now escapes all values including `=`s.
				"""
			pr_numbers: [18150]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			description: """
				VRL's `parse_nginx_log` function now handles more `combined` formats.
				"""
			contributors: ["scMarkus"]
			pr_numbers: [18150]
		},
		{
			type: "chore"
			scopes: ["vrl"]
			description: """
				VRL's `to_timestamp` function was deprecated.
				"""
			pr_numbers: [18150]
		},
		{
			type: "enhancement"
			scopes: ["vrl"]
			description: """
				VRL's `encrypt` and `decrypt` functions now support additional algorithms:

				- `CHACHA20-POLY1305`
				- `XCHACHA20-POLY1305`
				- `XSALSA20-POLY1305`
				- `AES-*-CTR-BE` (to disambiguate endianess of `AES-*-CTR`)
				- `AES-*-CTR-LE` (to disambiguate endianess of `AES-*-CTR`)
				"""
			contributors: ["alisa101rs"]
			pr_numbers: [18150]
		},
		{
			type: "fix"
			scopes: ["azure_blob_storage sink"]
			description: """
				The `azure_blob_storage` sink now sets the correct content-type based on the
				configured `encoding` options.
				"""
			contributors: ["stemjacobs"]
			pr_numbers: [18150]
		},
		{
			type: "fix"
			scopes: ["vector source"]
			description: """
				The `vector` source no longer fails to decode large payloads. This was a regression
				in 0.31.0 when a 4 MB limit was inadvertently applied.
				"""
			pr_numbers: [18186]
		},
		{
			type: "enhancement"
			scopes: ["nats source", "nats sink"]
			description: """
				The `nats` source and sink have been switched to use a more modern NATS library to
				lay the groundwork for a JetStream source.
				"""
			contributors: ["paolobarbolini", "makarchuk"]
			pr_numbers: [18165]
		},
	]

	commits: [
		{sha: "0fbdb335dd1cb9b467cc0280de314463ce108799", date: "2023-07-04 00:49:11 UTC", description: "Add macOS troubleshooting section to VRL web playground", pr_number:                        17824, scopes: ["docs"], type:                                                       "chore", breaking_change:       false, author: "Pavlos Rontidis", files_count:    1, insertions_count:  39, deletions_count:   2},
		{sha: "205300b4bea826d342d68153d0ee542857ee27ca", date: "2023-07-03 23:25:47 UTC", description: "Fix a couple typos with the registered event cache", pr_number:                             17809, scopes: ["observability"], type:                                              "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      1, insertions_count:  2, deletions_count:    2},
		{sha: "911477a191fe80d68203a3ab7669ce730cc0f43e", date: "2023-07-04 06:37:55 UTC", description: "combine build steps for integration test workflows", pr_number:                             17724, scopes: ["ci"], type:                                                         "enhancement", breaking_change: false, author: "neuronull", files_count:          17, insertions_count: 739, deletions_count:  234},
		{sha: "bc86222cd14327fdb459ceb0bb90e522aed3d2b3", date: "2023-07-05 06:19:54 UTC", description: "add fixed tag option to `RegisteredEventCache`", pr_number:                                 17814, scopes: ["observability"], type:                                              "enhancement", breaking_change: false, author: "Stephen Wakely", files_count:     5, insertions_count:  73, deletions_count:   12},
		{sha: "8519cb1f25a8d83dc014452db5cbdf6b08ee9c9e", date: "2023-07-06 01:36:10 UTC", description: "propagate and display invalid JSON errors in VRL web playground", pr_number:                17826, scopes: ["vrl"], type:                                                        "fix", breaking_change:         false, author: "Pavlos Rontidis", files_count:    3, insertions_count:  37, deletions_count:   17},
		{sha: "9581b35675ea89bc8fa016b451b948b18a9d19e1", date: "2023-07-06 01:38:57 UTC", description: "save time int test workflow merge queue", pr_number:                                        17869, scopes: ["ci"], type:                                                         "chore", breaking_change:       false, author: "neuronull", files_count:          1, insertions_count:  34, deletions_count:   32},
		{sha: "e9f21a98b9f17035fb971f3f95476ec37d9bbe56", date: "2023-07-06 03:54:19 UTC", description: "fix gardener issues comment workflow", pr_number:                                           17868, scopes: ["ci"], type:                                                         "chore", breaking_change:       false, author: "Doug Smith", files_count:         1, insertions_count:  28, deletions_count:   29},
		{sha: "9ec04438c9b59bc8ab8d4988c9f5744ad61c7248", date: "2023-07-06 06:12:39 UTC", description: "separate hanwritten and generated files in web-playground", pr_number:                      17871, scopes: [], type:                                                             "chore", breaking_change:       false, author: "Pavlos Rontidis", files_count:    5, insertions_count:  4, deletions_count:    151},
		{sha: "57ea2b3936c294b1b8b5911fd5f3742231147ea7", date: "2023-07-07 01:26:27 UTC", description: "fix gardener issues comment workflow pt 2", pr_number:                                      17886, scopes: ["ci"], type:                                                         "chore", breaking_change:       false, author: "Doug Smith", files_count:         1, insertions_count:  5, deletions_count:    1},
		{sha: "9c0d2f2a9bd0b50c5e1c703f4087b1e297c8ece6", date: "2023-07-06 23:38:33 UTC", description: "Bump Vector to 0.32.0", pr_number:                                                          17887, scopes: ["releasing"], type:                                                  "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      2, insertions_count:  2, deletions_count:    2},
		{sha: "99502bb3d7e9377b6e244d2eb248693c295c0386", date: "2023-07-07 00:54:53 UTC", description: "fix k8s validate comment job logic", pr_number:                                             17841, scopes: ["ci"], type:                                                         "chore", breaking_change:       false, author: "neuronull", files_count:          3, insertions_count:  25, deletions_count:   40},
		{sha: "1260c83e7e0222bd29f96c0533b6af6147c3c2da", date: "2023-07-07 00:21:25 UTC", description: "Fix link in v0.31.0 release docs", pr_number:                                               17888, scopes: ["releasing"], type:                                                  "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:  2, deletions_count:    2},
		{sha: "bc1b83ad51a5118aa6a7c3cab62dfb5eb3ce2c91", date: "2023-07-07 01:48:37 UTC", description: "Emit events with the `source_id` set", pr_number:                                           17870, scopes: ["lua transform"], type:                                              "fix", breaking_change:         false, author: "Bruce Guenter", files_count:      5, insertions_count:  190, deletions_count:  193},
		{sha: "0735ffe5b29f8603da9cc5f4fc017015c6529343", date: "2023-07-07 01:18:46 UTC", description: "Fix markdown syntax in minor release template", pr_number:                                  17890, scopes: ["releasing"], type:                                                  "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:  2, deletions_count:    2},
		{sha: "604fea0dcf54034dfab1ffcc27c12f0883c704e6", date: "2023-07-07 02:15:59 UTC", description: "Regenerate k8s manifests with v0.23.0 of the chart", pr_number:                             17892, scopes: ["releasing"], type:                                                  "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      18, insertions_count: 45, deletions_count:   42},
		{sha: "fa489f842b02fd7dd59a58e1339ae264050e92e4", date: "2023-07-07 06:31:17 UTC", description: "check VRL conditions return type at compile time", pr_number:                               17894, scopes: ["vrl"], type:                                                        "enhancement", breaking_change: false, author: "Pavlos Rontidis", files_count:    1, insertions_count:  10, deletions_count:   0},
		{sha: "f74d5dd39758eeb1adfb146dc517e3b3b7e1fda4", date: "2023-07-07 13:35:14 UTC", description: "Bump console-subscriber from 0.1.9 to 0.1.10", pr_number:                                   17844, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "51d849760824066f7ead64dc193831a5f85bdc14", date: "2023-07-07 13:35:20 UTC", description: "Bump paste from 1.0.12 to 1.0.13", pr_number:                                               17846, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:  5, deletions_count:    5},
		{sha: "ed5bc3afb2edb577c80bfdd6f0d7b11cf6f58b99", date: "2023-07-07 13:36:23 UTC", description: "Bump indoc from 2.0.1 to 2.0.2", pr_number:                                                 17843, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:  5, deletions_count:    5},
		{sha: "b2623165d0bb0f8732020e3bbd27db197cd780c1", date: "2023-07-07 15:23:45 UTC", description: "Bump serde_bytes from 0.11.9 to 0.11.11", pr_number:                                        17898, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  25, deletions_count:   25},
		{sha: "ea0f5b1e06f1e5e2eb22ef33168ad5ac862aaf63", date: "2023-07-07 15:28:03 UTC", description: "Bump thiserror from 1.0.40 to 1.0.43", pr_number:                                           17900, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:  4, deletions_count:    4},
		{sha: "4613b36284781d442728c05468ada320a92f71c0", date: "2023-07-07 15:43:27 UTC", description: "Bump ryu from 1.0.13 to 1.0.14", pr_number:                                                 17848, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:  2, deletions_count:    2},
		{sha: "93f81443d28524c47d17e42167208d7f44e8e7a0", date: "2023-07-07 15:49:02 UTC", description: "Bump colored from 2.0.0 to 2.0.4", pr_number:                                               17876, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  5, deletions_count:    5},
		{sha: "53b2854d95b2c4d06af0573ff9e02020e46653c5", date: "2023-07-07 15:53:47 UTC", description: "Bump async-trait from 0.1.68 to 0.1.71", pr_number:                                         17881, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "46dc18adcd73ebd97f069d58329704192b27e43e", date: "2023-07-07 16:00:17 UTC", description: "Bump smallvec from 1.10.0 to 1.11.0", pr_number:                                            17880, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:  2, deletions_count:    2},
		{sha: "bf1407c158b653fef810f4d8e570c93e47367c1c", date: "2023-07-07 16:02:25 UTC", description: "Bump enum_dispatch from 0.3.11 to 0.3.12", pr_number:                                       17879, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  4, deletions_count:    4},
		{sha: "bf2f97554f3696b0716210013e6dfde0bddbc958", date: "2023-07-07 18:08:18 UTC", description: "Bump inventory from 0.3.6 to 0.3.8", pr_number:                                             17842, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  3, deletions_count:    18},
		{sha: "17ccc56fadae5009541063b3780c603e945e38a1", date: "2023-07-07 18:11:54 UTC", description: "Bump bstr from 1.5.0 to 1.6.0", pr_number:                                                  17877, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  15, deletions_count:   9},
		{sha: "98ca6271cbc7c8c4fdabe309a2bf74f3eaca145a", date: "2023-07-08 00:24:41 UTC", description: "fix gardener issues comment workflow pt 3", pr_number:                                      17903, scopes: ["ci"], type:                                                         "chore", breaking_change:       false, author: "Doug Smith", files_count:         1, insertions_count:  21, deletions_count:   0},
		{sha: "01e2dfaa15ce62d156e92d8deac354cd40edf9e7", date: "2023-07-08 06:24:59 UTC", description: "describe the difference between configuration fields and runtime flags", pr_number:         17784, scopes: [], type:                                                             "docs", breaking_change:        false, author: "Dominic Burkart", files_count:    1, insertions_count:  19, deletions_count:   0},
		{sha: "4ef0b1778923567c8aa755e28d9419c52b6bc97c", date: "2023-07-08 03:25:09 UTC", description: "Add DataLoss error code as non-retryable", pr_number:                                       17904, scopes: ["vector sink"], type:                                                "fix", breaking_change:         false, author: "Scott Balmos", files_count:       1, insertions_count:  1, deletions_count:    0},
		{sha: "c4827e42a9bfe0f2ef2e0249593d39663ff2a490", date: "2023-07-08 01:46:19 UTC", description: "add spell check exception", pr_number:                                                      17906, scopes: ["spelling"], type:                                                   "fix", breaking_change:         false, author: "neuronull", files_count:          1, insertions_count:  1, deletions_count:    0},
		{sha: "251c4c4608a70fd6c112ecacd0517c301f21e33c", date: "2023-07-08 08:59:33 UTC", description: "Bump docker/setup-buildx-action from 2.8.0 to 2.9.0", pr_number:                            17907, scopes: ["ci"], type:                                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  4, deletions_count:    4},
		{sha: "70632b7d980a0721bec83124390eca3604baf2ee", date: "2023-07-08 02:48:08 UTC", description: "Remove path filter that runs all integration tests", pr_number:                             17908, scopes: ["ci"], type:                                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:  1, deletions_count:    2},
		{sha: "b10d0709b6d1746fe481f6299f1e5c8518489cfa", date: "2023-07-08 09:55:13 UTC", description: "Bump typetag from 0.2.8 to 0.2.9", pr_number:                                               17882, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  8, deletions_count:    8},
		{sha: "976580949148191ea6faabc7d77ddd60b3c33782", date: "2023-07-08 04:22:39 UTC", description: "check for team membership on secret-requiring int tests", pr_number:                        17909, scopes: ["ci"], type:                                                         "chore", breaking_change:       false, author: "neuronull", files_count:          1, insertions_count:  30, deletions_count:   8},
		{sha: "45e24c73e78d3daf609103635950245dcc715444", date: "2023-07-08 03:24:29 UTC", description: "cert verification with proxy enabled", pr_number:                                           17651, scopes: ["vector sink"], type:                                                "fix", breaking_change:         false, author: "joe miller", files_count:         1, insertions_count:  3, deletions_count:    18},
		{sha: "d592b0cf9d04de440e56a54687fd38bc33f1c3cd", date: "2023-07-08 04:41:24 UTC", description: "Swap out bloom crate for bloomy", pr_number:                                                17911, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      7, insertions_count:  35, deletions_count:   45},
		{sha: "c8deedab78cf45df70edb6ad8ee85fff6e888511", date: "2023-07-08 12:16:53 UTC", description: "Bump rdkafka from 0.32.2 to 0.33.2", pr_number:                                             17891, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "21267073cc957e9fb72a4fdf8f1e6b246344b0a9", date: "2023-07-08 12:17:15 UTC", description: "Bump clap_complete from 4.3.1 to 4.3.2", pr_number:                                         17878, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "cb950b0446b48f3c894a5913f7d4c416f0cbc47e", date: "2023-07-08 12:17:58 UTC", description: "Bump regex from 1.8.4 to 1.9.0", pr_number:                                                 17874, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:  19, deletions_count:   11},
		{sha: "97f4433f4689211877cf3042b5aaf14e38a32020", date: "2023-07-08 12:18:07 UTC", description: "Bump infer from 0.14.0 to 0.15.0", pr_number:                                               17860, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  4, deletions_count:    4},
		{sha: "fc62e9c80f63e77fa8ca8113e952b791db48dd86", date: "2023-07-08 12:53:24 UTC", description: "Bump bitmask-enum from 2.1.0 to 2.2.0", pr_number:                                          17833, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  4, deletions_count:    4},
		{sha: "ae59be62a3f8a87d5c12acbc8d60ed01b92e2ea3", date: "2023-07-08 12:54:40 UTC", description: "Bump schannel from 0.1.21 to 0.1.22", pr_number:                                            17850, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  4, deletions_count:    4},
		{sha: "17e6632739182cc03497d9711a0470656c848338", date: "2023-07-08 12:55:33 UTC", description: "Bump pin-project from 1.1.1 to 1.1.2", pr_number:                                           17837, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:  8, deletions_count:    8},
		{sha: "f91d1b204e3fd2ef4a464ba354aa6bb277e6a0a5", date: "2023-07-08 12:56:09 UTC", description: "Bump metrics-util from 0.15.0 to 0.15.1", pr_number:                                        17835, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  11, deletions_count:   20},
		{sha: "1a427ed2d33bfeefb2d3cbec814e3ab7a46d6e5e", date: "2023-07-08 13:01:09 UTC", description: "Bump serde_json from 1.0.99 to 1.0.100", pr_number:                                         17859, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:  7, deletions_count:    7},
		{sha: "528fac3d5155815e59563f01a10c6abcc6802006", date: "2023-07-08 06:24:43 UTC", description: "Fix integration test filter generation", pr_number:                                         17914, scopes: ["ci"], type:                                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:  1, deletions_count:    1},
		{sha: "0454d9dd938645af145001362908aa2a3342dc46", date: "2023-07-09 01:40:54 UTC", description: "Bump tokio from 1.29.0 to 1.29.1", pr_number:                                               17811, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    9, insertions_count:  14, deletions_count:   14},
		{sha: "c8e12672ffce5ba0ad1a948f0bcabf74ffab8f93", date: "2023-07-09 01:41:07 UTC", description: "Bump metrics from 0.21.0 to 0.21.1", pr_number:                                             17836, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:  6, deletions_count:    6},
		{sha: "44d3a8c9612897029406ba25f563e445ddb367d0", date: "2023-07-09 08:41:59 UTC", description: "Bump toml from 0.7.5 to 0.7.6", pr_number:                                                  17875, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:  14, deletions_count:   14},
		{sha: "bc5822c5017ecad6d59f720f3f874142287f3c6a", date: "2023-07-09 08:42:28 UTC", description: "Bump regex from 1.9.0 to 1.9.1", pr_number:                                                 17915, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:  10, deletions_count:   10},
		{sha: "49714cfa8a242e7b56acef645d1e82d675c8ffa4", date: "2023-07-11 00:57:19 UTC", description: "Bump snafu from 0.7.4 to 0.7.5", pr_number:                                                 17919, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    7, insertions_count:  10, deletions_count:   10},
		{sha: "6326f372c00431544a2f18456bab72188c1c0be9", date: "2023-07-11 00:57:47 UTC", description: "Bump bitmask-enum from 2.2.0 to 2.2.1", pr_number:                                          17921, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "22b6c2b9fa6c68b3ab7bbbc2b521c212eef66493", date: "2023-07-11 07:57:58 UTC", description: "Bump proc-macro2 from 1.0.63 to 1.0.64", pr_number:                                         17922, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:  66, deletions_count:   66},
		{sha: "8b2447a5ade93b3314876f5bc80429d9b6086f80", date: "2023-07-11 03:29:21 UTC", description: "address issues in integration test suite workflow", pr_number:                              17928, scopes: ["ci"], type:                                                         "fix", breaking_change:         false, author: "neuronull", files_count:          3, insertions_count:  26, deletions_count:   25},
		{sha: "37fb02ba114e86fa7aeb8f9ae54fc5daf724bc8c", date: "2023-07-11 02:48:39 UTC", description: "Remove mentions of deprecated transforms from guides", pr_number:                           17933, scopes: ["docs"], type:                                                       "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      2, insertions_count:  8, deletions_count:    7},
		{sha: "5b1219f17cb87c6e454f78011b666447d26e2cfd", date: "2023-07-11 09:55:33 UTC", description: "Bump async-compression from 0.4.0 to 0.4.1", pr_number:                                     17932, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  4, deletions_count:    4},
		{sha: "b535d184f864af5903e2f7f37671371a32aa2ff2", date: "2023-07-12 02:55:55 UTC", description: "Bump dashmap from 5.4.0 to 5.5.0", pr_number:                                               17938, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  11, deletions_count:   11},
		{sha: "d5b7fe6ab070ae85b23d3959aa18b218d2e968a4", date: "2023-07-12 02:58:10 UTC", description: "Bump apache-avro from 0.14.0 to 0.15.0", pr_number:                                         17931, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  31, deletions_count:   20},
		{sha: "784f3fed15c76e9c1416726e595c22ebd2c070f1", date: "2023-07-12 02:58:28 UTC", description: "Bump semver from 5.7.1 to 5.7.2 in /website", pr_number:                                    17937, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:  6, deletions_count:    6},
		{sha: "39897919be2402c13284bab27125b2b8a62225a6", date: "2023-07-12 02:58:49 UTC", description: "Bump serde from 1.0.167 to 1.0.168", pr_number:                                             17920, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:  11, deletions_count:   11},
		{sha: "98f44ae070ffdba58460e2262e0c70683fad3797", date: "2023-07-12 11:14:45 UTC", description: "Adding greptimedb metrics sink", pr_number:                                                 17198, scopes: ["new sink"], type:                                                   "feat", breaking_change:        false, author: "Ning Sun", files_count:           22, insertions_count: 1465, deletions_count: 1},
		{sha: "1acf5b47802bc83b4ded4bf2daf0c91f5502fb1b", date: "2023-07-12 08:01:21 UTC", description: "insert fields as event_path so names aren't parsed as a path", pr_number:                   17943, scopes: ["splunk_hec source"], type:                                          "fix", breaking_change:         false, author: "Stephen Wakely", files_count:     1, insertions_count:  24, deletions_count:   2},
		{sha: "f8461cbf356fe3c90d7d57a511c97d6fced31e47", date: "2023-07-12 00:24:52 UTC", description: "Add upgrade note for 0.31.0 about S3 path changes", pr_number:                              17934, scopes: ["releasing"], type:                                                  "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      2, insertions_count:  15, deletions_count:   0},
		{sha: "7774c495b7ab4d014a16dc036b284a5b723dc19b", date: "2023-07-12 01:20:39 UTC", description: "Use GitHub App token for team membership rather than user PAT", pr_number:                  17936, scopes: ["ci"], type:                                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      4, insertions_count:  30, deletions_count:   6},
		{sha: "7f459493d48165818ab8c0796ecea25742131703", date: "2023-07-12 10:06:51 UTC", description: "added sink review checklist", pr_number:                                                    17799, scopes: [], type:                                                             "chore", breaking_change:       false, author: "Stephen Wakely", files_count:     1, insertions_count:  38, deletions_count:   0},
		{sha: "77ffce8a47faeae64ca8d8eb6642c66f25f15c35", date: "2023-07-14 08:57:13 UTC", description: "Bump docker/setup-buildx-action from 2.9.0 to 2.9.1", pr_number:                            17955, scopes: ["ci"], type:                                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  4, deletions_count:    4},
		{sha: "a05542a1e392f0e18c8b305afd4d56bc146b6102", date: "2023-07-14 04:48:22 UTC", description: "stop ignoring topology test", pr_number:                                                    17953, scopes: [], type:                                                             "chore", breaking_change:       false, author: "Luke Steensen", files_count:      1, insertions_count:  0, deletions_count:    1},
		{sha: "d29424d95dbc7c9afd039890df38681ba309853f", date: "2023-07-14 08:48:40 UTC", description: "Migrate `LogSchema` `source_type_key` to new lookup code", pr_number:                       17947, scopes: ["config"], type:                                                     "feat", breaking_change:        false, author: "Pavlos Rontidis", files_count:    35, insertions_count: 240, deletions_count:  140},
		{sha: "3921a24e13b6558db8aec29f19fcd68a1601460c", date: "2023-07-14 10:16:41 UTC", description: "Bump to syn 2, serde_with 3, darling 0.20, and serde_derive_internals 0.28", pr_number:     17930, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "Doug Smith", files_count:         15, insertions_count: 195, deletions_count:  114},
		{sha: "467baab82cab45acc84d3f3f962c4fbda4f3f632", date: "2023-07-14 14:17:04 UTC", description: "Bump governor from 0.5.1 to 0.6.0", pr_number:                                              17960, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "4d4b393e1eb9ad02b5c1bfad41d5317e6f26b09a", date: "2023-07-14 14:17:45 UTC", description: "Bump lru from 0.10.1 to 0.11.0", pr_number:                                                 17945, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "50736e2ed463bef20985329ee5c59d7261b070d8", date: "2023-07-14 12:10:15 UTC", description: "Fix schema.log_namespace and telemetry.tags documentation", pr_number:                      17961, scopes: [], type:                                                             "docs", breaking_change:        false, author: "Spencer Gilbert", files_count:    2, insertions_count:  22, deletions_count:   10},
		{sha: "4a377a79f184c1f09ca5d516712257101a838a2b", date: "2023-07-14 21:36:38 UTC", description: "Bump serde_json from 1.0.100 to 1.0.102", pr_number:                                        17948, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:  7, deletions_count:    7},
		{sha: "f4b11115c2245836d2bc607b07b2556e012871d3", date: "2023-07-14 21:37:07 UTC", description: "Bump typetag from 0.2.9 to 0.2.10", pr_number:                                              17968, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  6, deletions_count:    6},
		{sha: "656b1fe18f0750a6c4d705bb29a771251c0a6b88", date: "2023-07-15 04:37:22 UTC", description: "Bump darling from 0.20.1 to 0.20.3", pr_number:                                             17969, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:  12, deletions_count:   12},
		{sha: "5dfede4784c7a9457d2a15ad51f1ac13bcc6730c", date: "2023-07-15 04:42:02 UTC", description: "Bump syn from 2.0.23 to 2.0.25", pr_number:                                                 17970, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:  27, deletions_count:   27},
		{sha: "eb4383fce9e539bd72eb711bd825d542afb20cec", date: "2023-07-15 01:28:49 UTC", description: "Add `--features` with default features for vdev test", pr_number:                           17977, scopes: ["vdev"], type:                                                       "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      1, insertions_count:  7, deletions_count:    6},
		{sha: "66f483874b137c786765e2f8635f7a74b76c7c1a", date: "2023-07-15 04:54:52 UTC", description: "Bump serde from 1.0.168 to 1.0.171", pr_number:                                             17976, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:  11, deletions_count:   11},
		{sha: "38719a3b459fa9bf34552edc7deaf3a023b5257a", date: "2023-07-15 04:54:56 UTC", description: "Bump lapin from 2.2.1 to 2.3.1", pr_number:                                                 17974, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "5dd208424c4acb6c0cb0dab5b9b5768cc83daf37", date: "2023-07-15 05:08:29 UTC", description: "Mark loki-logproto crate as unpublished", pr_number:                                        17979, scopes: ["dev"], type:                                                        "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      2, insertions_count:  365, deletions_count:  0},
		{sha: "fde77bdd9c3acbbf84309b9dcd49d65eea394517", date: "2023-07-17 21:49:53 UTC", description: "Bump assert_cmd from 2.0.11 to 2.0.12", pr_number:                                          17982, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "81de3e54bfdbd112b4177db907e542bf540d97b0", date: "2023-07-18 04:50:47 UTC", description: "Bump dyn-clone from 1.0.11 to 1.0.12", pr_number:                                           17987, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:  5, deletions_count:    5},
		{sha: "9a6ffad33b128b37dae15dc161529112be19f6bc", date: "2023-07-17 23:40:52 UTC", description: "Bump anyhow from 1.0.71 to 1.0.72", pr_number:                                              17986, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:  5, deletions_count:    5},
		{sha: "a1d3c3a8488e05dc66f3661ca5ee48a27ca7eb95", date: "2023-07-18 01:08:14 UTC", description: "Correct docs for `syslog_ip`", pr_number:                                                   18003, scopes: ["docs", "syslog source"], type:                                      "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      1, insertions_count:  1, deletions_count:    1},
		{sha: "04f9ddce818f8f09499824be166ff7313a533e0e", date: "2023-07-18 02:49:44 UTC", description: "Bump serde_bytes from 0.11.11 to 0.11.12", pr_number:                                       17988, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "fbc03080515a8f14492b80c92b2fa5b38c62d639", date: "2023-07-18 02:49:59 UTC", description: "Bump proc-macro2 from 1.0.64 to 1.0.66", pr_number:                                         17989, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:  68, deletions_count:   68},
		{sha: "536a7f12cbeef373979f845ef3f1b565463cbccd", date: "2023-07-18 11:10:15 UTC", description: "make `database` and `table` templateable", pr_number:                                       18005, scopes: ["clickhouse sink"], type:                                            "feat", breaking_change:        false, author: "Doug Smith", files_count:         7, insertions_count:  193, deletions_count:  49},
		{sha: "a36d36e862a598a5f825b034f97971e6d7967ba7", date: "2023-07-18 15:42:10 UTC", description: "Bump paste from 1.0.13 to 1.0.14", pr_number:                                               17991, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:  5, deletions_count:    5},
		{sha: "0ebe7a7e0db26a4b88f9b3d3cabd35cf0279b810", date: "2023-07-18 15:42:23 UTC", description: "Bump serde_json from 1.0.102 to 1.0.103", pr_number:                                        17992, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:  7, deletions_count:    7},
		{sha: "f53c6877eaf7c794b906f7f06ea3c1ab67c223f6", date: "2023-07-18 15:42:35 UTC", description: "Bump ryu from 1.0.14 to 1.0.15", pr_number:                                                 17993, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:  2, deletions_count:    2},
		{sha: "caf61032dd077516353957cd3959ec34e6333cf1", date: "2023-07-18 15:42:46 UTC", description: "Bump syn from 2.0.25 to 2.0.26", pr_number:                                                 17994, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:  27, deletions_count:   27},
		{sha: "9c59feaf08336123ff45a66b0cfa115523c010aa", date: "2023-07-18 15:42:57 UTC", description: "Bump inventory from 0.3.8 to 0.3.9", pr_number:                                             17995, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "90f494c6b2a0df5c4d3c41aa94ed60fc8e219841", date: "2023-07-18 15:43:49 UTC", description: "Bump opendal from 0.38.0 to 0.38.1", pr_number:                                             17999, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:  2, deletions_count:    2},
		{sha: "60e765db182c568380849fc50396101f2b5476e9", date: "2023-07-18 15:44:00 UTC", description: "Bump uuid from 1.4.0 to 1.4.1", pr_number:                                                  18001, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:  2, deletions_count:    2},
		{sha: "52ac10ac23fe120ac3e1c89ec3196be9ac894009", date: "2023-07-18 15:44:11 UTC", description: "Bump axum from 0.6.18 to 0.6.19", pr_number:                                                18002, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "6e552f01449bc55572314fa5d4853662126e538d", date: "2023-07-18 21:09:59 UTC", description: "Bump quote from 1.0.29 to 1.0.31", pr_number:                                               17990, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:  71, deletions_count:   71},
		{sha: "3c257589dac737fcc245485d860b12b5ba7b2830", date: "2023-07-18 21:10:09 UTC", description: "Bump indoc from 2.0.2 to 2.0.3", pr_number:                                                 17996, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:  5, deletions_count:    5},
		{sha: "ca368d8c6b9d67d79efe059336770522e410e057", date: "2023-07-19 04:10:19 UTC", description: "Bump semver from 1.0.17 to 1.0.18", pr_number:                                              17998, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  5, deletions_count:    5},
		{sha: "39a2bf56e4d8bdf23caedb177ad6c25ac439c28d", date: "2023-07-19 04:10:31 UTC", description: "Bump serde_with from 3.0.0 to 3.1.0", pr_number:                                            18004, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:  12, deletions_count:   12},
		{sha: "115bd7b4dc4f065a99bb4e3dc464141026e6b3bf", date: "2023-07-19 07:33:57 UTC", description: "Fix \"Bring your own toolbox\" in `DEVELOPING.md`", pr_number:                              18014, scopes: [], type:                                                             "docs", breaking_change:        false, author: "Chris Sinjakli", files_count:     1, insertions_count:  5, deletions_count:    4},
		{sha: "fd10e69a3bb0880798ff3690db08050740e51084", date: "2023-07-19 03:37:39 UTC", description: "Install script supports Apple ARM with Rosetta", pr_number:                                 18016, scopes: [], type:                                                             "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:    2, insertions_count:  12, deletions_count:   9},
		{sha: "32950d8ddb5623637a84103dce5e4f3ac176ab3b", date: "2023-07-19 04:13:28 UTC", description: "Migrate LogSchema::host_key to new lookup code", pr_number:                                 17972, scopes: ["config"], type:                                                     "feat", breaking_change:        false, author: "Pavlos Rontidis", files_count:    39, insertions_count: 306, deletions_count:  201},
		{sha: "b44a431bd188ca191b5b9c89d8485010bb2cd747", date: "2023-07-19 04:54:27 UTC", description: "Fix `interval` fractional second parsing", pr_number:                                       17917, scopes: ["demo gcp_pubsub internal_metrics source throttle transform"], type: "fix", breaking_change:         false, author: "Scott Balmos", files_count:       5, insertions_count:  8, deletions_count:    8},
		{sha: "3b9166249742a9dc114235550995eecf25288e64", date: "2023-07-19 10:58:40 UTC", description: "Bump serde_yaml from 0.9.22 to 0.9.24", pr_number:                                          18007, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  8, deletions_count:    8},
		{sha: "b00727ee13cc4eef6dde63bb8eaa8e0a570294ce", date: "2023-07-19 07:10:35 UTC", description: "restart api server based on topology", pr_number:                                           17958, scopes: ["reload"], type:                                                     "fix", breaking_change:         false, author: "KH-Moogsoft", files_count:        3, insertions_count:  50, deletions_count:   9},
		{sha: "7d0db6bbf33a7bc2e929d5d56b207dce42da4317", date: "2023-07-20 03:47:17 UTC", description: "Install dd-rust-license-tool from crates.io", pr_number:                                    18025, scopes: ["dev"], type:                                                        "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      5, insertions_count:  6, deletions_count:    6},
		{sha: "52a8036722ab5cd4ed92d8916b89d85d6447f8c0", date: "2023-07-20 04:57:47 UTC", description: "make tests deterministic through absolute comparisons instead of bounds checks", pr_number: 17956, scopes: ["component validation"], type:                                       "fix", breaking_change:         false, author: "neuronull", files_count:          10, insertions_count: 426, deletions_count:  444},
		{sha: "aa605206baaa6db0506ed0698cfd14847abbb5a9", date: "2023-07-20 07:04:14 UTC", description: "validate `component_errors_total` for sources", pr_number:                                  17965, scopes: ["component validation"], type:                                       "feat", breaking_change:        false, author: "neuronull", files_count:          5, insertions_count:  24, deletions_count:   5},
		{sha: "752056c06ae926b61ab33ea8d53dafd1e4f04f16", date: "2023-07-21 04:38:33 UTC", description: "Bump zstd from 0.12.3+zstd.1.5.2 to 0.12.4", pr_number:                                     18031, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  7, deletions_count:    7},
		{sha: "b36c5311c7f5787ea0770e83df3ce3ae5c7a7e0b", date: "2023-07-21 12:16:33 UTC", description: "Bump serde from 1.0.171 to 1.0.173", pr_number:                                             18032, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    9, insertions_count:  12, deletions_count:   11},
		{sha: "7050b7ef4b73f0997e3f69be12ec34547f6e6ecb", date: "2023-07-21 13:39:59 UTC", description: "Bump serde_yaml from 0.9.24 to 0.9.25", pr_number:                                          18040, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  8, deletions_count:    8},
		{sha: "0bf6abd03fc92c80f306a20da9825c8298efe041", date: "2023-07-21 16:14:54 UTC", description: "count byte_size after transforming event", pr_number:                                       17941, scopes: ["observability"], type:                                              "chore", breaking_change:       false, author: "Stephen Wakely", files_count:     29, insertions_count: 427, deletions_count:  155},
		{sha: "81f5c507793d73a0678968c4a596b213cfa5c619", date: "2023-07-22 06:32:37 UTC", description: "consolidate `EventCountTags` with `TaggedEventsSent`", pr_number:                           17865, scopes: ["observability"], type:                                              "chore", breaking_change:       false, author: "Stephen Wakely", files_count:     12, insertions_count: 62, deletions_count:   83},
		{sha: "9b4cd44d599b08f3459fb1108b86921eb76a355d", date: "2023-07-21 22:52:09 UTC", description: "Bump tower-http from 0.4.1 to 0.4.2", pr_number:                                            18030, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  6, deletions_count:    20},
		{sha: "4de89f23e7ead95e96d82334bd0815ce33359927", date: "2023-07-22 05:52:22 UTC", description: "Bump num-traits from 0.2.15 to 0.2.16", pr_number:                                          18039, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  4, deletions_count:    4},
		{sha: "983a92a8b7eeab3b262c02557ccc1cbd5f11d75e", date: "2023-07-22 05:52:33 UTC", description: "Bump syn from 2.0.26 to 2.0.27", pr_number:                                                 18042, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:  27, deletions_count:   27},
		{sha: "689a79e20e0130fd2070be28173fa3ef565b27ac", date: "2023-07-22 02:58:03 UTC", description: "add support for `external_id` in AWS assume role", pr_number:                               17743, scopes: ["provider aws"], type:                                               "feat", breaking_change:        false, author: "Ankit Luthra", files_count:       11, insertions_count: 162, deletions_count:  1},
		{sha: "0f14c0d02d5f9bc4ed68236d07d74a70eab13c64", date: "2023-07-22 04:14:59 UTC", description: "Migrate LogSchema::message_key to new lookup code", pr_number:                              18024, scopes: ["config"], type:                                                     "feat", breaking_change:        false, author: "Pavlos Rontidis", files_count:    55, insertions_count: 639, deletions_count:  410},
		{sha: "250cc950b0d3feb27755b614bb3402543195a683", date: "2023-07-22 03:20:04 UTC", description: "Fix links in CONTRIBUTING.md", pr_number:                                                   18061, scopes: ["docs"], type:                                                       "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:  4, deletions_count:    4},
		{sha: "5bccafe44931a12695f7ab0ba20e177a65fb2454", date: "2023-07-22 05:54:18 UTC", description: "Bump typetag from 0.2.10 to 0.2.11", pr_number:                                             18048, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  6, deletions_count:    6},
		{sha: "437cad6fcc99266c92aa228269787e3b18a79c45", date: "2023-07-22 12:55:13 UTC", description: "Bump serde from 1.0.173 to 1.0.174", pr_number:                                             18050, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:  11, deletions_count:   11},
		{sha: "bbe2c74de044cf33ce0cd371c6bfff00c1f285ad", date: "2023-07-22 12:55:46 UTC", description: "Bump async-trait from 0.1.71 to 0.1.72", pr_number:                                         18053, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "497fdcede4ae828a00574c496122752b2a70e89c", date: "2023-07-22 12:55:58 UTC", description: "Bump rmp-serde from 1.1.1 to 1.1.2", pr_number:                                             18054, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "f1d4196d295ae0e188ab4b2ca9ea2e4165467745", date: "2023-07-22 12:56:10 UTC", description: "Bump tower-http from 0.4.2 to 0.4.3", pr_number:                                            18055, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "087a0ace58867c6152317360717e3c97f8e143be", date: "2023-07-22 12:56:21 UTC", description: "Bump nkeys from 0.3.0 to 0.3.1", pr_number:                                                 18056, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  5, deletions_count:    4},
		{sha: "b305334b99a2d3cefcc0dd48e6e60b371645a24d", date: "2023-07-23 07:01:15 UTC", description: "Bump security-framework from 2.9.1 to 2.9.2", pr_number:                                    18051, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "ee2396ff926468ed94199a930f3e04db2e7bbd04", date: "2023-07-23 07:01:25 UTC", description: "Bump thiserror from 1.0.43 to 1.0.44", pr_number:                                           18052, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:  4, deletions_count:    4},
		{sha: "684e43f5bb2be30a1bb63a742dbc6f6215604f37", date: "2023-07-23 07:01:38 UTC", description: "Bump inventory from 0.3.9 to 0.3.10", pr_number:                                            18064, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "a9df9589b9ba1869ae354fea48419786fa41468e", date: "2023-07-24 22:48:02 UTC", description: "run requests in parallel with timeouts", pr_number:                                         18021, scopes: ["prometheus_scrape source"], type:                                   "enhancement", breaking_change: false, author: "Renning Bruns", files_count:      7, insertions_count:  124, deletions_count:  17},
		{sha: "db9e47fef445ece5c86d786c3cf96049d8f6ee6b", date: "2023-07-25 01:43:12 UTC", description: "Add licenses to packages", pr_number:                                                       18006, scopes: [], type:                                                             "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      20, insertions_count: 404, deletions_count:  11},
		{sha: "86636020f145bae0e0259b78cc9ffa789381505e", date: "2023-07-25 05:37:00 UTC", description: "Migrate LogSchema::metadata key to new lookup code", pr_number:                             18058, scopes: ["config"], type:                                                     "feat", breaking_change:        false, author: "Pavlos Rontidis", files_count:    3, insertions_count:  56, deletions_count:   33},
		{sha: "fecca5ef183268f0034995a695e3424d8a86fd03", date: "2023-07-25 02:52:01 UTC", description: "Upgrade debian usages to use bookworm", pr_number:                                          18057, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      7, insertions_count:  10, deletions_count:   9},
		{sha: "16a42ed29c832a39021b2822072f8a67d72ce7a8", date: "2023-07-25 06:58:22 UTC", description: "Bump serde from 1.0.174 to 1.0.175", pr_number:                                             18071, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:  11, deletions_count:   11},
		{sha: "d8f211eaa2b9d5c27089c17dfbbd762de167a988", date: "2023-07-25 06:58:35 UTC", description: "Bump inventory from 0.3.10 to 0.3.11", pr_number:                                           18070, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "dc2348a8028c399ffef8939ad27161a7e5c62ef2", date: "2023-07-25 12:58:47 UTC", description: "Bump quote from 1.0.31 to 1.0.32", pr_number:                                               18069, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:  71, deletions_count:   71},
		{sha: "3968325707f90937e91b0ba12a6dbdae4719854b", date: "2023-07-25 12:59:49 UTC", description: "Bump tokio-tungstenite from 0.19.0 to 0.20.0", pr_number:                                   18065, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:  10, deletions_count:   13},
		{sha: "1dd505fde140b0d64431346bfc72ee24144b8710", date: "2023-07-25 21:59:12 UTC", description: "Update to Rust 1.71.0", pr_number:                                                          18075, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      22, insertions_count: 34, deletions_count:   37},
		{sha: "421b421bb988335316417c80129014ff80179246", date: "2023-07-25 23:02:19 UTC", description: "Update tokio-util fork to 0.7.8", pr_number:                                                18078, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      2, insertions_count:  3, deletions_count:    3},
		{sha: "c592cb17dc4fd153804335c4b315f43d22f0bceb", date: "2023-07-26 06:36:07 UTC", description: "add more direct regression case for s3 sink", pr_number:                                    18082, scopes: [], type:                                                             "chore", breaking_change:       false, author: "Luke Steensen", files_count:      3, insertions_count:  54, deletions_count:   0},
		{sha: "b85f4f9cda826e08767c69dcffde04ffad977932", date: "2023-07-26 04:47:11 UTC", description: "send encoded message as binary frame", pr_number:                                           18060, scopes: ["websocket sink"], type:                                             "fix", breaking_change:         false, author: "Zhong Chen", files_count:         1, insertions_count:  18, deletions_count:   1},
		{sha: "f6c53d035e5c8d2c655c7c8b7ad82f7f341f6862", date: "2023-07-26 06:54:50 UTC", description: "Bump roaring from 0.10.1 to 0.10.2", pr_number:                                             18079, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "b70074cb73cb03a44909d80295828c46fc74f4de", date: "2023-07-26 12:54:57 UTC", description: "Bump typetag from 0.2.11 to 0.2.12", pr_number:                                             18066, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  6, deletions_count:    6},
		{sha: "065eecbcafd37a99ba8667f69cee78d96bb132e1", date: "2023-07-27 04:55:30 UTC", description: "replace LogEvent 'String's with '&OwnedTargetPath's", pr_number:                            18084, scopes: ["config"], type:                                                     "feat", breaking_change:        false, author: "Pavlos Rontidis", files_count:    9, insertions_count:  142, deletions_count:  106},
		{sha: "28f5c23aa84f70736fe5ef5132e274b3611cceb9", date: "2023-07-28 03:06:22 UTC", description: "replace tuples with &OwnedTargetPath wherever possible", pr_number:                         18097, scopes: ["config"], type:                                                     "feat", breaking_change:        false, author: "Pavlos Rontidis", files_count:    19, insertions_count: 67, deletions_count:   112},
		{sha: "f015b299b0249d082f297f7aee15f42ae091c77b", date: "2023-07-28 03:41:10 UTC", description: "Refactor TraceEvent insert to use TargetPath compatible types", pr_number:                  18090, scopes: ["config"], type:                                                     "feat", breaking_change:        false, author: "Pavlos Rontidis", files_count:    8, insertions_count:  66, deletions_count:   56},
		{sha: "a8bb9f45867ab2435640258ad07babc1d0b8f747", date: "2023-07-28 06:52:23 UTC", description: "LogSchema metadata key refacoring", pr_number:                                              18099, scopes: ["config"], type:                                                     "feat", breaking_change:        false, author: "Pavlos Rontidis", files_count:    6, insertions_count:  40, deletions_count:   37},
		{sha: "00ed120317b8673952fec5b8bad3baba482854f7", date: "2023-07-28 07:17:56 UTC", description: "Bump serde_json from 1.0.103 to 1.0.104", pr_number:                                        18095, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    6, insertions_count:  7, deletions_count:    7},
		{sha: "9458b6c63d5fa069ab4cb956c7044eb9f74ebfbe", date: "2023-07-28 07:18:09 UTC", description: "Bump no-proxy from 0.3.2 to 0.3.3", pr_number:                                              18094, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  4, deletions_count:    4},
		{sha: "4915b429a81887736fd1864cd45697f052105277", date: "2023-07-28 23:47:21 UTC", description: "add tests to sinks for Data Volume tags", pr_number:                                        17853, scopes: ["observability"], type:                                              "chore", breaking_change:       false, author: "Stephen Wakely", files_count:     14, insertions_count: 499, deletions_count:  121},
		{sha: "8a2f8f67cd23fde5c7a48c07c5f67c67b833c089", date: "2023-07-28 23:44:43 UTC", description: "allow empty message_key value in config", pr_number:                                        18091, scopes: ["config"], type:                                                     "fix", breaking_change:         false, author: "Pavlos Rontidis", files_count:    1, insertions_count:  12, deletions_count:   5},
		{sha: "a06c71102867af5e4526e445f9ba8f4506382a30", date: "2023-07-29 10:59:05 UTC", description: "add support for protobuf decoding", pr_number:                                              18019, scopes: ["codecs"], type:                                                     "feat", breaking_change:        false, author: "Daniel599", files_count:          32, insertions_count: 742, deletions_count:  1},
		{sha: "b009e4d72c7cf0864e5cd5dcb6a392e6559db786", date: "2023-08-01 07:17:17 UTC", description: "Update syslog_loose to properly handle escapes", pr_number:                                 18114, scopes: ["codecs"], type:                                                     "chore", breaking_change:       false, author: "Stephen Wakely", files_count:     3, insertions_count:  16, deletions_count:   5},
		{sha: "d3e512881b2f7e7135c4b0fd917ac501815086c4", date: "2023-08-01 06:21:02 UTC", description: "Bump syn from 2.0.27 to 2.0.28", pr_number:                                                 18117, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    1, insertions_count:  27, deletions_count:   27},
		{sha: "48abad44407f36c494611a87a6698d909eb8a839", date: "2023-08-01 12:21:33 UTC", description: "Bump redis from 0.23.0 to 0.23.1", pr_number:                                               18107, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  16, deletions_count:   3},
		{sha: "564104eadbe5bcc230497ee22edc37039fd21bb2", date: "2023-08-01 12:21:47 UTC", description: "Bump tikv-jemallocator from 0.5.0 to 0.5.4", pr_number:                                     18102, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  4, deletions_count:    4},
		{sha: "e6f2cccc9dcb93d537dfa2aad5741a6c1c7bac6a", date: "2023-08-01 14:18:27 UTC", description: "Bump serde from 1.0.175 to 1.0.180", pr_number:                                             18127, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:  11, deletions_count:   11},
		{sha: "36111b5e7f971b336244113762210a486fdd6d0f", date: "2023-08-02 06:48:27 UTC", description: "fix issues when using container tools and `cargo` is not installed locally", pr_number:     18112, scopes: ["dev"], type:                                                        "fix", breaking_change:         false, author: "Hugo Hromic", files_count:        2, insertions_count:  18, deletions_count:   3},
		{sha: "36788d13bd9f87c480c47677d0ca5f2ba400d743", date: "2023-08-01 22:49:03 UTC", description: "Move protobuf codec options under a `protobuf` key", pr_number:                             18111, scopes: ["codecs"], type:                                                     "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      19, insertions_count: 327, deletions_count:  223},
		{sha: "93b19459010575a702a0a5eba7c2bb923bf5baa1", date: "2023-08-02 02:22:26 UTC", description: "fix gardener move blocked to triage on comment", pr_number:                                 18126, scopes: ["ci"], type:                                                         "fix", breaking_change:         false, author: "neuronull", files_count:          1, insertions_count:  1, deletions_count:    1},
		{sha: "7df6af7cf5866e2f49b657b5ae3ec54521810e32", date: "2023-08-02 06:55:40 UTC", description: "Bump async_graphql, async_graphql_warp from 5.0.10 to 6.0.0", pr_number:                    18122, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "neuronull", files_count:          7, insertions_count:  98, deletions_count:   24},
		{sha: "5a6ce731c999f0960e8411a9b286730314c4e7ac", date: "2023-08-03 02:25:04 UTC", description: "Fix basic sink tutorial issues", pr_number:                                                 18136, scopes: ["internal docs"], type:                                              "docs", breaking_change:        false, author: "Tom de Bruijn", files_count:      2, insertions_count:  2, deletions_count:    2},
		{sha: "8068f1d115666adafb95dac50ecc2a8879f1af8a", date: "2023-08-03 02:16:31 UTC", description: "replace path tuples with actual target paths", pr_number:                                   18139, scopes: ["config"], type:                                                     "chore", breaking_change:       false, author: "Pavlos Rontidis", files_count:    17, insertions_count: 137, deletions_count:  409},
		{sha: "8022464f8ae08b68b3ae571a90fdf50ca6822973", date: "2023-08-03 03:35:18 UTC", description: "propagate config build error instead of panicking", pr_number:                              18124, scopes: ["config"], type:                                                     "fix", breaking_change:         false, author: "Pavlos Rontidis", files_count:    23, insertions_count: 82, deletions_count:   60},
		{sha: "600f8191a8fe169eb38c429958dd59714349acb4", date: "2023-08-03 04:09:58 UTC", description: "Refactor top and tap for library use", pr_number:                                           18129, scopes: ["api"], type:                                                        "chore", breaking_change:       false, author: "Will Wang", files_count:          8, insertions_count:  215, deletions_count:  154},
		{sha: "34eaf43d37b51703510045890bbb279d7e0bf78e", date: "2023-08-04 06:53:00 UTC", description: "exclude protobuf files from spell checking", pr_number:                                     18152, scopes: ["ci"], type:                                                         "chore", breaking_change:       false, author: "neuronull", files_count:          1, insertions_count:  1, deletions_count:    0},
		{sha: "a7c95ddf287fb3f97f41cb662d07113ed5ddec73", date: "2023-08-04 06:53:11 UTC", description: "Bump indicatif from 0.17.5 to 0.17.6", pr_number:                                           18146, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  4, deletions_count:    4},
		{sha: "b2d23a838e7b5409273d82afa647b960b24499d3", date: "2023-08-05 01:53:13 UTC", description: "update sink tutorials with Data Volume tag changes", pr_number:                             18148, scopes: ["external docs"], type:                                              "chore", breaking_change:       false, author: "Stephen Wakely", files_count:     2, insertions_count:  37, deletions_count:   14},
		{sha: "adfef2eeca6e4047e372e530109d640e55b38478", date: "2023-08-05 05:28:07 UTC", description: "Update VRL to 0.6.0", pr_number:                                                            18150, scopes: ["deps", "vrl"], type:                                                "feat", breaking_change:        false, author: "Pavlos Rontidis", files_count:    16, insertions_count: 277, deletions_count:  110},
		{sha: "0ddd221f4f657801955102aba76c7f36db68f9fe", date: "2023-08-05 06:35:00 UTC", description: "Bump axum from 0.6.19 to 0.6.20", pr_number:                                                18154, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "2c51c5c5a0daf75803cd417781bd0e318d1ab9da", date: "2023-08-05 06:35:03 UTC", description: "Bump serde from 1.0.180 to 1.0.181", pr_number:                                             18155, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:  11, deletions_count:   11},
		{sha: "be551c8c231d6c874edad0de5bdd9c14e6bdfb63", date: "2023-08-05 12:39:51 UTC", description: "Bump serde_with from 3.1.0 to 3.2.0", pr_number:                                            18162, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    4, insertions_count:  13, deletions_count:   12},
		{sha: "e125eee58eab3660dc203ff92653e7bd10229845", date: "2023-08-08 04:27:25 UTC", description: "Bump pin-project from 1.1.2 to 1.1.3", pr_number:                                           18169, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:  8, deletions_count:    8},
		{sha: "09610b3d8a998ca51db2823e4d39fd41071f385e", date: "2023-08-08 00:34:44 UTC", description: "Bump openssl from 0.10.55 to 0.10.56", pr_number:                                           18170, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  6, deletions_count:    6},
		{sha: "6036d5c8235dad865c1f32374726a618543bd046", date: "2023-08-08 06:53:35 UTC", description: "Bump serde from 1.0.181 to 1.0.183", pr_number:                                             18171, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    8, insertions_count:  11, deletions_count:   11},
		{sha: "3c535ecc289f2376133c8229ecc8316dbb4806bf", date: "2023-08-08 10:53:14 UTC", description: "switch to crates.io release of Azure SDK", pr_number:                                       18166, scopes: ["azure provider"], type:                                             "feat", breaking_change:        false, author: "Paolo Barbolini", files_count:    3, insertions_count:  34, deletions_count:   52},
		{sha: "8fc574f98baf0551de8439eaf9ade1a3dea6f37c", date: "2023-08-08 02:34:11 UTC", description: "Fix TOML parsing of compression levels", pr_number:                                         18173, scopes: ["config"], type:                                                     "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      1, insertions_count:  33, deletions_count:   3},
		{sha: "0ae3d513711491fd50037a40b4741e3e1a52773d", date: "2023-08-08 23:59:21 UTC", description: "Bump clap from 4.3.19 to 4.3.21", pr_number:                                                18178, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    7, insertions_count:  20, deletions_count:   20},
		{sha: "3b53bcda04a06b365bc35965e8934eddac1b7fc2", date: "2023-08-09 01:18:48 UTC", description: "support configuring the initial ARC limit", pr_number:                                      18175, scopes: ["adaptive_concurrency"], type:                                       "feat", breaking_change:        false, author: "Blake Mealey", files_count:       42, insertions_count: 457, deletions_count:  1},
		{sha: "e476e120503d8682c8aef511b7af9b8851f2d03c", date: "2023-08-09 02:46:05 UTC", description: "Refactor 'event.get()' to use path types", pr_number:                                       18160, scopes: ["config"], type:                                                     "feat", breaking_change:        false, author: "Pavlos Rontidis", files_count:    13, insertions_count: 124, deletions_count:  59},
		{sha: "4a049d4a90a6a994c530140236c7d67e516674e3", date: "2023-08-09 03:37:40 UTC", description: "Base Content-Type on encoder and not compression", pr_number:                               18184, scopes: ["azure_blob sink"], type:                                            "fix", breaking_change:         false, author: "Steve Jacobs", files_count:       3, insertions_count:  11, deletions_count:   19},
		{sha: "d8eefe331af0faa478b9fe2f58de2a25a83589e9", date: "2023-08-09 05:45:04 UTC", description: "replace various string paths with actual paths", pr_number:                                 18109, scopes: [], type:                                                             "chore", breaking_change:       false, author: "Pavlos Rontidis", files_count:    24, insertions_count: 206, deletions_count:  149},
		{sha: "0c1cf23f4563e0a0beb6e080915da8ef5f78e7e7", date: "2023-08-09 05:49:27 UTC", description: "make LogEvent index operator test only", pr_number:                                         18185, scopes: ["config"], type:                                                     "fix", breaking_change:         false, author: "Pavlos Rontidis", files_count:    1, insertions_count:  1, deletions_count:    0},
		{sha: "4cc9cdf04cbd2e25426ca3283b76c5b3eee93565", date: "2023-08-09 03:48:13 UTC", description: "Remove the 4MB default for requests", pr_number:                                            18186, scopes: ["vector source"], type:                                              "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:      1, insertions_count:  3, deletions_count:    1},
		{sha: "0aeb143cd8012e17f125569e84b968228ec4b4a1", date: "2023-08-10 05:49:39 UTC", description: "refactor to new sink style", pr_number:                                                     18172, scopes: ["azure_monitor_logs sink"], type:                                    "chore", breaking_change:       false, author: "Doug Smith", files_count:         6, insertions_count:  939, deletions_count:  695},
		{sha: "caf6103f76ce7cd913129f64e0d5c5d17bdbc799", date: "2023-08-10 05:52:31 UTC", description: "emit an error if the condition return type is not a boolean", pr_number:                    18196, scopes: ["vrl"], type:                                                        "feat", breaking_change:        false, author: "Pavlos Rontidis", files_count:    1, insertions_count:  1, deletions_count:    1},
		{sha: "8454a6f46099e95f6aef41a0830cda6bb3b22b0e", date: "2023-08-10 05:09:04 UTC", description: "Bump OpenSSL base version to 3.1.*", pr_number:                                             17669, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      6, insertions_count:  56, deletions_count:   10},
		{sha: "92c2b9cce248c250b962f4a1de1194e14f177ce3", date: "2023-08-11 01:09:45 UTC", description: "Bump tokio from 1.29.1 to 1.30.0", pr_number:                                               18202, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    9, insertions_count:  15, deletions_count:   16},
		{sha: "cd8c8b18eed10ccb59e6929d7ee30feac2a6ec25", date: "2023-08-10 23:30:53 UTC", description: "Expose shutdown errors", pr_number:                                                         18153, scopes: ["core"], type:                                                       "chore", breaking_change:       false, author: "Bruce Guenter", files_count:      10, insertions_count: 175, deletions_count:  109},
		{sha: "7603d2813e389a1103286c634d92d9da1e8a8b52", date: "2023-08-11 01:28:43 UTC", description: "Update `smp` to its latest released version", pr_number:                                    18204, scopes: [], type:                                                             "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count: 1, insertions_count:  1, deletions_count:    6},
		{sha: "91e48f6834ee51ec2492080e8ebc21d380ee5a4b", date: "2023-08-11 03:40:57 UTC", description: "Upgrading version of lading used", pr_number:                                               18210, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:  1, deletions_count:    1},
		{sha: "f39a0e96cf18ffeb225908e21c7255d3d8550898", date: "2023-08-11 05:14:16 UTC", description: "Fix package install in Tiltfile", pr_number:                                                18198, scopes: ["dev"], type:                                                        "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      3, insertions_count:  14, deletions_count:   3},
		{sha: "f77fd3d2735dbfeda3d9bdaf8f11605e4acd8a33", date: "2023-08-12 04:56:35 UTC", description: "fix Rust toolchain check in Makefile", pr_number:                                           18218, scopes: ["dev"], type:                                                        "fix", breaking_change:         false, author: "Hugo Hromic", files_count:        1, insertions_count:  1, deletions_count:    1},
		{sha: "483e46fe4656d3636d6cbff18c2e9f86baa48d68", date: "2023-08-12 06:39:06 UTC", description: "migrate to `async_nats` client", pr_number:                                                 18165, scopes: ["nats source", "nats sink"], type:                                   "enhancement", breaking_change: false, author: "Paolo Barbolini", files_count:    7, insertions_count:  155, deletions_count:  245},
		{sha: "eaed0a899a22d5ab23ac4eb0ab23cc34280fb5da", date: "2023-08-11 23:09:47 UTC", description: "Upgrade to Rust 1.71.1", pr_number:                                                         18221, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      2, insertions_count:  2, deletions_count:    2},
		{sha: "ca7fa05ca98ac8ed097dc7a24b1652d62dbf283a", date: "2023-08-12 04:59:30 UTC", description: "Refactor dnstap to use 'OwnedValuePath's", pr_number:                                       18212, scopes: ["config"], type:                                                     "feat", breaking_change:        false, author: "Pavlos Rontidis", files_count:    4, insertions_count:  868, deletions_count:  1195},
		{sha: "ad08d010fbeb2df02e38433064916d8ee8bc37b3", date: "2023-08-12 04:28:55 UTC", description: "Run hadolint on distributed Dockerfiles", pr_number:                                        18224, scopes: ["releasing"], type:                                                  "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      5, insertions_count:  11, deletions_count:   4},
		{sha: "00037b075e1842139eb5a6c97eabfb09042c95e7", date: "2023-08-12 05:42:19 UTC", description: "Bump regex from 1.9.1 to 1.9.3", pr_number:                                                 18167, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    5, insertions_count:  15, deletions_count:   15},
		{sha: "01295b0beab4d0a7b13c52515d1120618879dc97", date: "2023-08-12 06:18:39 UTC", description: "Remove an unneeded advisory ignore", pr_number:                                             18226, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      1, insertions_count:  0, deletions_count:    5},
		{sha: "8bbe6a6f0c2a3cd3c97ec0495cbc067c88918264", date: "2023-08-12 13:37:36 UTC", description: "Bump strip-ansi-escapes from 0.1.1 to 0.2.0", pr_number:                                    18203, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    3, insertions_count:  24, deletions_count:   5},
		{sha: "8838faff9e29dab975580200c571b18b970696c6", date: "2023-08-12 06:43:34 UTC", description: "Swap tui crate for ratatui", pr_number:                                                     18225, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:      6, insertions_count:  30, deletions_count:   43},
		{sha: "e61c14fdf8111530324878235deedb33526bb897", date: "2023-08-12 14:27:02 UTC", description: "Bump gloo-utils from 0.1.7 to 0.2.0", pr_number:                                            18227, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "ec3b4401bb839d207b30ab9561533c958dcc4f99", date: "2023-08-15 03:54:29 UTC", description: "Bump redis from 0.23.1 to 0.23.2", pr_number:                                               18234, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "20fa1bfc7d4edcb67d26d798249abdd767ba2b72", date: "2023-08-15 03:54:44 UTC", description: "Bump async-trait from 0.1.72 to 0.1.73", pr_number:                                         18235, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "851e99ca77ade46fe2a01320db8d16e6bf610c00", date: "2023-08-15 03:54:55 UTC", description: "Bump bitmask-enum from 2.2.1 to 2.2.2", pr_number:                                          18236, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
		{sha: "cb007fea2ee81882943dec0c52e2883bb5a9de86", date: "2023-08-15 03:55:35 UTC", description: "Bump log from 0.4.19 to 0.4.20", pr_number:                                                 18237, scopes: ["deps"], type:                                                       "chore", breaking_change:       false, author: "dependabot[bot]", files_count:    2, insertions_count:  3, deletions_count:    3},
	]
}
