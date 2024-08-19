package metadata

releases: "0.38.0": {
	date:     "2024-05-07"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.38.0!

		Be sure to check out the [upgrade guide](/highlights/2024-05-07-0-38-0-upgrade-guide) for
		breaking changes in this release.

		This release just contains a mix of small enhancements and bug fixes. See the changelog
		below.
		"""

	changelog: [
		{
			type: "enhancement"
			description: """
				The Google Chronicle Unstructured Log sink now supports adding a namespace to the log events for indexing within Chronicle.
				"""
			contributors: ["ChocPanda"]
		},
		{
			type: "enhancement"
			description: """
				A new histogram metric was added, `component_received_bytes`, that measures the byte-size of individual events received by the following sources:

				- `socket`
				- `statsd`
				- `syslog`
				"""
			contributors: ["pabloem"]
		},
		{
			type: "chore"
			description: """
				The `protobuf` decoder will no longer set fields on the decoded event that are not set in the incoming byte stream. Previously, it would set the default value for the field even if it wasn't in the event. This change ensures that the encoder will return the exact same bytes for the same given event.
				"""
		},
		{
			type: "feat"
			description: """
				Support was added for additional config options for `length_delimited` framing.
				"""
			contributors: ["esensar"]
		},
		{
			type: "enhancement"
			description: """
				A new option, `max_number_of_messages`, was added to the SQS configuration of the `aws_s3` source.
				"""
			contributors: ["fdamstra"]
		},
		{
			type: "enhancement"
			description: """
				Support was added for `expiration_ms` on the `amqp` sink, to set an expiration on messages sent.
				"""
			contributors: ["sonnens"]
		},
		{
			type: "fix"
			description: """
				The `kafka` source again emits received bytes and event counts correctly.
				"""
			contributors: ["jches"]
		},
		{
			type: "fix"
			description: """
				An issue was fixed where the `log_to_metric` transform with `all_metrics` set to `true` failed to convert properly-formatted 'set'-type events into metrics.
				"""
			contributors: ["pabloem"]
		},
		{
			type: "enhancement"
			description: """
				The previous inner databend client was changed to client provided by databend rust driver in https://github.com/datafuselabs/bendsql/. With the new client, the `endpoint` config supports both HTTP URI like `http://localhost:8000` as well as DSN like `databend://root:@localhost:8000/mydatabase?sslmode=disable&arg=value` which could provide more customization for the inner client.
				"""
			contributors: ["everpcpc"]
		},
		{
			type: "enhancement"
			description: """
				The distroless images have changed their base from Debian 11 to Debian 12.
				"""
		},
		{
			type: "enhancement"
			description: """
				The Google Chronicle Unstructured Log sink now supports adding labels to the log events for indexing within Chronicle.
				"""
			contributors: ["bwerthmann"]
		},
		{
			type: "fix"
			description: """
				Vector would panic when attempting to use a combination af `access_key_id` and
				`assume_role` authentication with the AWS components. This error has now been
				fixed.
				"""
			contributors: ["StephenWakely"]
		},
		{
			type: "fix"
			description: """
				A `message` semantic meaning was added to the Splunk HEC source. This only applies to the `Vector` log namespace.
				"""
		},
		{
			type: "enhancement"
			description: """
				A new config field `missing_field_as` was added to the `databend` sink to specify the behavior when fields are missing. Previously the behavior was the same as setting this new configuration option to `ERROR`. The new default value is `NULL`.
				"""
			contributors: ["everpcpc"]
		},
		{
			type: "feat"
			description: """
				The `clickhouse` sink now has a new configuration option, `insert_random_shard`, to tell Clickhouse to insert into a random shard (by setting `insert_distributed_one_random_shard`). See the Clickhouse [Distributed Table Engine docs](https://clickhouse.com/docs/en/engines/table-engines/special/distributed) for details.
				"""
			contributors: ["rguleryuz"]
		},
		{
			type: "chore"
			description: """
				Previously the `datadog_agent` setting `parse_ddtags` parsed the tag string into an Object. It is now parsed into an Array of `key:value` strings, which matches the  behavior of the Datadog logs backend intake.
				"""
		},
		{
			type: "fix"
			description: """
				The `datadog_logs` sink was not re-constructing ddtags that may have been parsed upstream by the `datadog_agent` source's `parse_ddtags` setting. The sink log encoding was fixed to re-assemble the tags into a unified string that the Datadog logs intake expects.
				"""
		},
		{
			type: "feat"
			description: """
				The Elasticsearch sink is now able to use [external versioning for documents](https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-index_.html#index-versioning). To use it set `bulk.version_type` to `external` and then set `bulk.version` to either some static value like `123` or use templating to use an actual field from the document `{{ my_document_field }}`.
				"""
			contributors: ["radimsuckr"]
		},
		{
			type: "fix"
			description: """
				The `prometheus_exporter` sink is now able to correctly handle a mix of both incremental and
				absolute valued gauges arriving for the same metric series.
				"""
			contributors: ["RussellRollins"]
		},
		{
			type: "fix"
			description: """
				Previously, when the `auto_extract_timestamp` setting in the `splunk_hec_logs` Sink was enabled, the sink was attempting to remove the existing event timestamp. This would throw a warning that the timestamp type was invalid.

				This has been fixed to leave the timestamp on the event if `auto_extract_timestamp` is enabled, since this setting indicates that Vector should let Splunk remove it.
				"""
		},
		{
			type: "chore"
			description: """
				The deprecated `--strict-env-vars` flag has been removed. The previous behavior of defaulting unset
				environment variables can be accomplished by syntax like `${FOO-}` (which will default `FOO` to
				empty string if unset). See the [configuration environment variables
				docs](https://vector.dev/docs/reference/configuration/#environment-variables) for more about this
				syntax.
				"""
			breaking: true
		},
		{
			type: "chore"
			description: """
				The `enterprise` global configuration has been deprecated and will be removed in
				a future version. This corresponds to a deprecation for "Vector Enterprise" by the
				Datadog Observability Pipelines product.
				"""
			breaking: true
		},
		{
			type: "feat"
			description: """
				Support was added for loading secrets from AWS Secrets Manager (AWS SSM).
				"""
			breaking: true
		},
	]

	commits: [
		{sha: "c7dde0312a6d04201eb9641fe8b8cc8967ffd3fe", date: "2024-03-27 05:17:16 UTC", description: "add options to `length_delimited` framing", pr_number: 20154, scopes: ["codecs"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 37, insertions_count: 920, deletions_count: 95},
		{sha: "5349313dacc77f6798b51574c0639220b58f4284", date: "2024-03-26 23:43:01 UTC", description: "expose semantic meaning log event helper fn", pr_number: 20178, scopes: ["core"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "c1fc9f03b816c54b0efb57198e5a16159a324ac9", date: "2024-03-27 01:25:15 UTC", description: "Bump Vector to 0.38.0", pr_number: 20180, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "3378adad7b5b1819ffd52cf9563f519bb13ebdad", date: "2024-03-27 01:39:59 UTC", description: "Bump Kubernetes manifsts to chart version 0.32.0", pr_number: 20182, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "41bb21ef711d55884b02eee42b0126c25e97dd5e", date: "2024-03-28 02:33:57 UTC", description: "Bump MSRV to reduce usage of `async_trait`", pr_number: 20155, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 31, insertions_count: 18, deletions_count: 56},
		{sha: "c7f0a85fbfc6bdcf17c5a3bf1ad571c80731b701", date: "2024-03-29 05:32:39 UTC", description: "support LogNamespace in Component Validation Framework", pr_number: 20148, scopes: ["testing"], type: "chore", breaking_change: false, author: "neuronull", files_count: 14, insertions_count: 211, deletions_count: 160},
		{sha: "742b883b5881b1b1f88d01c023c277b293500ee3", date: "2024-03-29 08:49:53 UTC", description: "Bump temp-dir from 0.1.12 to 0.1.13", pr_number: 20151, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "cf61b90a8cb6c80a541fad0689867f8ae55bae5a", date: "2024-03-29 08:49:57 UTC", description: "Bump bollard from 0.16.0 to 0.16.1", pr_number: 20158, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "3c4eb68abb16c81fa7170926256785433b6a3553", date: "2024-03-29 15:50:02 UTC", description: "Bump indoc from 2.0.4 to 2.0.5", pr_number: 20159, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "461597721247195853910c3b3d7cba9a8a16b3cc", date: "2024-03-29 15:50:06 UTC", description: "Bump indexmap from 2.2.5 to 2.2.6", pr_number: 20161, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 23, deletions_count: 23},
		{sha: "04813b9eee62d7dc3c55b761b144478af73a1a95", date: "2024-03-29 15:50:14 UTC", description: "Bump async-trait from 0.1.78 to 0.1.79", pr_number: 20163, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "52a15a0548a03baa59eb4e107bf5a4041505bd29", date: "2024-03-29 15:50:18 UTC", description: "Bump syn from 2.0.53 to 2.0.55", pr_number: 20164, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 47, deletions_count: 47},
		{sha: "e25efa1c270d5cb008240edb4ad2413535a332e7", date: "2024-03-29 15:50:22 UTC", description: "Bump serde_yaml from 0.9.33 to 0.9.34+deprecated", pr_number: 20165, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 10, deletions_count: 10},
		{sha: "1befcd9a6f8d0115fcc11ac866ba380c9893ee25", date: "2024-03-29 15:50:26 UTC", description: "Bump arc-swap from 1.7.0 to 1.7.1", pr_number: 20166, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "656e2207c2f3057d72c19667b1673f621be56ade", date: "2024-03-29 15:50:41 UTC", description: "Bump bufbuild/buf-breaking-action from 1.1.3 to 1.1.4", pr_number: 20171, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "b05fb602bb734adb82f17032e91c44ae9fc5a4df", date: "2024-03-29 15:51:10 UTC", description: "Bump express from 4.18.2 to 4.19.2 in /website", pr_number: 20183, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 24, deletions_count: 19},
		{sha: "8a78b8270b61136a37e7a8b5c364b912e6515d2b", date: "2024-03-29 15:51:14 UTC", description: "Bump graphql_client from 0.13.0 to 0.14.0", pr_number: 20187, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "06f3ad3416b4afe89a02fb29704cda40f8e71da3", date: "2024-03-29 16:24:42 UTC", description: "Bump the aws group with 1 update", pr_number: 20175, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "eb7ab42940d1743ad2fa2f44318c0b25240f3595", date: "2024-03-30 00:03:40 UTC", description: "Bump os_info from 3.8.1 to 3.8.2", pr_number: 20162, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "6c3003dc0592f9fb7ff121a971db9af44c793a68", date: "2024-03-30 07:03:47 UTC", description: "Bump regex from 1.10.3 to 1.10.4", pr_number: 20168, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 6, deletions_count: 6},
		{sha: "5a6b670ce05cc7c34b0af0cf2a58810d47b2f71c", date: "2024-03-30 07:03:51 UTC", description: "Bump the clap group with 1 update", pr_number: 20176, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 15, deletions_count: 15},
		{sha: "a8e17a5e46ccdf55826afa6057fc4e6c1347f017", date: "2024-04-01 21:54:32 UTC", description: "Bump quanta from 0.12.2 to 0.12.3", pr_number: 20218, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "fead132341ac52d487abd85bb74b4cab57eafaff", date: "2024-04-01 21:55:24 UTC", description: "Bump bytes from 1.5.0 to 1.6.0", pr_number: 20167, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 82, deletions_count: 82},
		{sha: "173deda35dbf90377a82aacbc5cca273e0468e73", date: "2024-04-02 04:55:39 UTC", description: "Bump serde_json from 1.0.114 to 1.0.115", pr_number: 20188, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "5ffb1a5557f1a257d841687b4d3ba48547dfd8d4", date: "2024-04-02 04:55:42 UTC", description: "Bump actions/add-to-project from 0.6.1 to 1.0.0", pr_number: 20194, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "0599d60758386b631533adce250b4820f5434e59", date: "2024-04-02 04:56:54 UTC", description: "Bump the zstd group with 1 update", pr_number: 20199, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 13, deletions_count: 13},
		{sha: "ae5673367d3db51700b3c1b3264859f4f18cc2db", date: "2024-04-02 04:57:02 UTC", description: "Bump memchr from 2.7.1 to 2.7.2", pr_number: 20200, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "be56139a9c662ac09bd294f801b93073cccb0754", date: "2024-04-02 04:57:12 UTC", description: "Bump enum_dispatch from 0.3.12 to 0.3.13", pr_number: 20207, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "4b4068fa3d0a1372b3d6a90c4b5ae73da3fc0f02", date: "2024-04-02 04:57:23 UTC", description: "Bump tokio from 1.36.0 to 1.37.0", pr_number: 20208, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 10, insertions_count: 13, deletions_count: 13},
		{sha: "333ed14f71c1acaaeb0936f4a3cdefd9e89518f9", date: "2024-04-02 04:57:39 UTC", description: "Bump syn from 2.0.55 to 2.0.57", pr_number: 20219, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 47, deletions_count: 47},
		{sha: "f2f16b61825be20c9d9e15328d33880c6addaa9a", date: "2024-04-02 07:37:14 UTC", description: "fix example kustomization file", pr_number: 20085, scopes: ["kubernetes platform"], type: "docs", breaking_change: false, author: "Dylan Werner-Meier", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "9e7f6e3a7b095bcea285bd02740c5bc5605e0f08", date: "2024-04-02 02:50:44 UTC", description: "don't attempt to remove timestamp if auto extract is enabled", pr_number: 20213, scopes: ["splunk_hec_logs sink"], type: "fix", breaking_change: false, author: "neuronull", files_count: 5, insertions_count: 73, deletions_count: 32},
		{sha: "044e29daf4b57832481224703c1c6e085c05da80", date: "2024-04-02 09:47:25 UTC", description: "Bump chrono from 0.4.34 to 0.4.37", pr_number: 20195, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 15, deletions_count: 25},
		{sha: "562f7b780ad339f933d49ee119b2646467a401f5", date: "2024-04-02 20:46:05 UTC", description: "Bump async-compression from 0.4.6 to 0.4.7", pr_number: 20221, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "667331435254c84ebe8a3d2b5173cebfcbcc7507", date: "2024-04-02 22:51:25 UTC", description: "Bump hostname from 0.3.1 to 0.4.0", pr_number: 20222, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 37, deletions_count: 16},
		{sha: "38f4868a9e35dade00098ff71bf5c3c294c335d0", date: "2024-04-03 06:12:28 UTC", description: "properly encode all semantically defined DD reserved attributes", pr_number: 20226, scopes: ["datadog_logs sink"], type: "chore", breaking_change: false, author: "neuronull", files_count: 5, insertions_count: 144, deletions_count: 68},
		{sha: "f1fdfd0a3c00227544d250654ee567925c0a0a79", date: "2024-04-04 11:24:01 UTC", description: "Correct docker.md so the command can be executed", pr_number: 20227, scopes: [], type: "docs", breaking_change: false, author: "slamp", files_count: 1, insertions_count: 6, deletions_count: 5},
		{sha: "7b85728c474abc2ff691624ac253ff1777d450b7", date: "2024-04-05 01:11:13 UTC", description: "support log namespaced host and timestamp attributes", pr_number: 20211, scopes: ["splunk_hec_logs sink"], type: "chore", breaking_change: false, author: "neuronull", files_count: 11, insertions_count: 231, deletions_count: 113},
		{sha: "95a987bd718c89254dff6460947097f3c90d41a9", date: "2024-04-09 00:12:48 UTC", description: "make consts public", pr_number: 20257, scopes: ["datadog"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 2},
		{sha: "c9a686473bc544acc63cb3148dba3cab46b8f5b0", date: "2024-04-09 06:37:58 UTC", description: "Update pacman.md as vector is available now in extra repository", pr_number: 20241, scopes: [], type: "docs", breaking_change: false, author: "slamp", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "59b3d7467777992e94c193d4e32321b179d20b22", date: "2024-04-09 12:44:37 UTC", description: "remove unnecessary clone", pr_number: 20245, scopes: [], type: "chore", breaking_change: false, author: "baoyachi", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "9080d053edff3aa520be857f9619eac5345b8654", date: "2024-04-09 06:41:13 UTC", description: "fix source span instrumentation", pr_number: 20242, scopes: ["kafka source"], type: "fix", breaking_change: false, author: "j chesley", files_count: 2, insertions_count: 21, deletions_count: 9},
		{sha: "05502862d63dd27cf316560ed6d35b627147bc3f", date: "2024-04-10 12:31:18 UTC", description: "service use databend-client", pr_number: 20244, scopes: ["databend sink"], type: "feat", breaking_change: false, author: "everpcpc", files_count: 13, insertions_count: 181, deletions_count: 504},
		{sha: "b63443213f4efb7129e253b397f84c3bf552c41a", date: "2024-04-09 23:45:35 UTC", description: "set not working in log-to-metric transform when all_metrics=true", pr_number: 20228, scopes: ["log_to_metric transform"], type: "fix", breaking_change: false, author: "Pablo", files_count: 3, insertions_count: 119, deletions_count: 0},
		{sha: "d8b67177e8544020a85cdc080cac5ee5f6328bae", date: "2024-04-09 23:59:11 UTC", description: "normalizer doesn't handle mix of absolute and incremental metrics", pr_number: 20193, scopes: ["metrics"], type: "fix", breaking_change: false, author: "Luke Steensen", files_count: 2, insertions_count: 126, deletions_count: 63},
		{sha: "b460f7406c3480da5347f1e9753129173658555e", date: "2024-04-09 23:01:52 UTC", description: "Bump security-framework from 2.9.2 to 2.10.0", pr_number: 20220, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "1c42df30138bee5ce93c60ef76ac39979e507591", date: "2024-04-10 06:02:02 UTC", description: "Bump the aws group with 1 update", pr_number: 20229, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "ad71ab38615fd2bd23165fe0ab5a11d803bc163e", date: "2024-04-10 06:02:21 UTC", description: "Bump bufbuild/buf-setup-action from 1.30.0 to 1.30.1", pr_number: 20238, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "c3c6edc65a41e61ceac50d2d3d0d335f02acadb8", date: "2024-04-10 06:02:31 UTC", description: "Bump cached from 0.49.2 to 0.49.3", pr_number: 20247, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "94ed61e0d17359fccd66fb8b3d36d935df6caae3", date: "2024-04-10 06:02:44 UTC", description: "Bump mock_instant from 0.3.2 to 0.4.0", pr_number: 20249, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "21578e786bc9293f3a1f59c1eb11a6e59a70e821", date: "2024-04-10 06:03:02 UTC", description: "Bump warp from 0.3.6 to 0.3.7", pr_number: 20253, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 38, deletions_count: 9},
		{sha: "7b2d389bc3502108c6de0d3432d6c8cf537508f5", date: "2024-04-10 06:03:25 UTC", description: "Bump docker/setup-buildx-action from 3.2.0 to 3.3.0", pr_number: 20260, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "28403059420fb3fe6f8188dc0b63db6c41c61d53", date: "2024-04-10 06:03:35 UTC", description: "Bump crc from 3.0.1 to 3.2.1", pr_number: 20263, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "eedc623bfc0663f9240bd10f30748c6a3e3e3611", date: "2024-04-10 06:03:47 UTC", description: "Bump getrandom from 0.2.12 to 0.2.14", pr_number: 20264, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 15, deletions_count: 15},
		{sha: "e4e7321e7e0a872612458d6f9d1ec083126b76be", date: "2024-04-10 06:17:06 UTC", description: "Bump the prost group with 4 updates", pr_number: 20248, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 44, deletions_count: 45},
		{sha: "665ab39dce1cfcad46cbae746c82973df957bfdf", date: "2024-04-10 20:37:20 UTC", description: "Download SHA256SUMS to correct location", pr_number: 20269, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "7d7b1a2620c65540183298aac804aa4f7abe48ec", date: "2024-04-11 17:48:51 UTC", description: "add expiration option to AMQP messages", pr_number: 20215, scopes: ["amqp sink"], type: "enhancement", breaking_change: false, author: "John Sonnenschein", files_count: 3, insertions_count: 14, deletions_count: 2},
		{sha: "f1439bc42e8a9498b169c14eb030d8d6f2530ac8", date: "2024-04-11 22:57:13 UTC", description: "Adding a histogram for event byte size", pr_number: 19686, scopes: ["source metrics"], type: "enhancement", breaking_change: false, author: "Pablo", files_count: 6, insertions_count: 21, deletions_count: 1},
		{sha: "153919d77e6efa24d7b7573e00b42ce5a0e9b747", date: "2024-04-12 07:05:01 UTC", description: "Adds an option `max_number_of_messages` for the aws_s3 source", pr_number: 20261, scopes: ["aws_s3 source"], type: "enhancement", breaking_change: false, author: "Fred Damstra", files_count: 4, insertions_count: 46, deletions_count: 1},
		{sha: "fae2ebfcb53035eeccdf399837228c7373d3007f", date: "2024-04-13 05:05:05 UTC", description: "add semantic meaning for Vector log namespace", pr_number: 20292, scopes: ["splunk hec"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 5, deletions_count: 1},
		{sha: "1b0bdcf022397f78df5522fe369eb457e5bae9dc", date: "2024-04-15 09:11:53 UTC", description: "use http client when building assume role for AccessKey", pr_number: 20285, scopes: ["aws service"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 2, insertions_count: 52, deletions_count: 20},
		{sha: "304ed46976c71e3f1313ab9f3233c14e32ed59dc", date: "2024-04-17 00:09:33 UTC", description: "Bump distroless base image to debian12 from debian11", pr_number: 20267, scopes: ["releasing"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 1},
		{sha: "cf11a013d292512d98c6d5f534a7463087dc55ba", date: "2024-04-17 05:51:14 UTC", description: "Run unit test tests in CI", pr_number: 20313, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 53, deletions_count: 2},
		{sha: "45a853c7245c6eea5099541c0bf28c8e22819d4b", date: "2024-04-17 20:44:43 UTC", description: "Bump serde from 1.0.197 to 1.0.198", pr_number: 20321, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "7869a7895d202d3f6eecbf70d40074995947c0dd", date: "2024-04-18 03:45:13 UTC", description: "Bump rmpv from 1.0.1 to 1.0.2", pr_number: 20318, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "8a20f055b4f799b6ddd318d1ae29b6c47b6d5421", date: "2024-04-18 06:37:02 UTC", description: "allow external document versioning", pr_number: 20102, scopes: ["elasticsearch sink"], type: "feat", breaking_change: false, author: "Radim Sückr", files_count: 8, insertions_count: 412, deletions_count: 23},
		{sha: "5a4a2b2a10131af7ef4ca32ff13b9040e231f5a6", date: "2024-04-18 05:26:10 UTC", description: "Bump serde_json from 1.0.115 to 1.0.116", pr_number: 20320, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "a568cf90a4b369a31a1d4d17c581ae2a01976082", date: "2024-04-19 03:34:56 UTC", description: "Bump async-compression from 0.4.7 to 0.4.8", pr_number: 20255, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "d6ae1ae5c68875f079d482c9a95dd47582613a77", date: "2024-04-19 03:35:07 UTC", description: "Bump clap_complete from 4.5.1 to 4.5.2 in the clap group", pr_number: 20272, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "a56da92ffce00913092135ca05a1c441a4fd28bb", date: "2024-04-19 03:35:18 UTC", description: "Bump rstest from 0.18.2 to 0.19.0", pr_number: 20273, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "d2c88090322006584b5e9152ba4d635c38dc8ef5", date: "2024-04-19 12:39:14 UTC", description: "add config missing_field_as for ndjson insert", pr_number: 20331, scopes: ["databend sink"], type: "feat", breaking_change: false, author: "everpcpc", files_count: 7, insertions_count: 66, deletions_count: 5},
		{sha: "954af0f2942edbd68f54bef95abcb5797c23d905", date: "2024-04-19 04:41:06 UTC", description: "Bump mlua from 0.9.6 to 0.9.7", pr_number: 20251, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "cf6469e0c6bb2ee12b0a3e33aef718203cd2dda6", date: "2024-04-19 23:16:42 UTC", description: "Bump hickory-proto from 0.24.0 to 0.24.1", pr_number: 20341, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "8c124ac162d22e6b7ffe50b13f385d78b0cb9f11", date: "2024-04-19 23:16:52 UTC", description: "Bump syslog from 6.1.0 to 6.1.1", pr_number: 20340, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "14e9b478913bda2ffc53899618c4ff317f922929", date: "2024-04-20 06:17:07 UTC", description: "Bump syn from 2.0.57 to 2.0.60", pr_number: 20329, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 118, deletions_count: 118},
		{sha: "cf7542d38deba3c44a6066961cc12d9f62f57916", date: "2024-04-20 06:17:54 UTC", description: "Bump rmp-serde from 1.1.2 to 1.2.0", pr_number: 20319, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "b4b96aa47feda7101e93dd55367f41d0d9147b4a", date: "2024-04-20 01:32:41 UTC", description: "update rustls crate for RUSTSEC-2024-0336", pr_number: 20343, scopes: ["security"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 15, deletions_count: 15},
		{sha: "431a3c034c1316d0d1d6265a96d36fa3462f7cba", date: "2024-04-20 08:33:46 UTC", description: "Bump windows-service from 0.6.0 to 0.7.0", pr_number: 20317, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "1049616f522f08d2045b840012541805203b7e3f", date: "2024-04-20 08:33:59 UTC", description: "Bump ratatui from 0.26.1 to 0.26.2", pr_number: 20310, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "ceb50fba5daf43a8765303b89d09f2c44c399071", date: "2024-04-20 08:34:18 UTC", description: "Bump the aws group with 3 updates", pr_number: 20299, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 13, deletions_count: 9},
		{sha: "32dedb474a981d71a98d521c7416317aa8a600a3", date: "2024-04-20 08:34:28 UTC", description: "Bump actions/add-to-project from 1.0.0 to 1.0.1", pr_number: 20295, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "9e03021fcb4e714d6d9dd326434f6711691cee70", date: "2024-04-20 08:34:53 UTC", description: "Bump nkeys from 0.4.0 to 0.4.1", pr_number: 20289, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 5},
		{sha: "cff9c88c44bf3e7447801addee729779ccced34a", date: "2024-04-20 04:15:35 UTC", description: "Bump rust-toolchain to 1.77.2", pr_number: 20344, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "177cd0e7ca174e5c8e9ba9b60c7faa728bbd4981", date: "2024-04-22 20:53:12 UTC", description: "Bump async-trait from 0.1.79 to 0.1.80", pr_number: 20290, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "eaacd2f2763519397850df42ce209ccc977ecfcc", date: "2024-04-22 21:00:50 UTC", description: "Bump quote from 1.0.35 to 1.0.36", pr_number: 20274, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 75, deletions_count: 75},
		{sha: "f391fe2d2f3e8c3c5462d908d08813ff1a47ff66", date: "2024-04-23 04:00:55 UTC", description: "Bump anyhow from 1.0.81 to 1.0.82", pr_number: 20275, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "0efb52048217904862ad3b5264ebe742951c69f9", date: "2024-04-23 02:36:34 UTC", description: "Include cgroups2 root memory metrics", pr_number: 20294, scopes: ["host_metrics source"], type: "fix", breaking_change: false, author: "Benjamin Werthmann", files_count: 1, insertions_count: 3, deletions_count: 7},
		{sha: "6cf2e8e015a652da463c2c7786144a74703a9e87", date: "2024-04-23 00:20:51 UTC", description: "Bump fakedata_generator from 0.4.0 to 0.5.0", pr_number: 20351, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "8197f4eedd3f3415f83effe2fbb23b69e63fa6d6", date: "2024-04-23 07:20:59 UTC", description: "Bump thiserror from 1.0.58 to 1.0.59", pr_number: 20352, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "334913e8860e7a167d6e90662bc6fe83deffdf5d", date: "2024-04-23 07:21:09 UTC", description: "Bump cargo_toml from 0.19.2 to 0.20.0", pr_number: 20350, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c211ca0c799caff292975452ba4bd5e8dbaeaaf5", date: "2024-04-23 20:49:01 UTC", description: "Bump the aws group across 1 directory with 4 updates", pr_number: 20359, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 12, deletions_count: 12},
		{sha: "25b22d7ee272176704826003f8266b2b012d8a46", date: "2024-04-24 11:52:36 UTC", description: "Update docker.md", pr_number: 20346, scopes: [], type: "docs", breaking_change: false, author: "Erlang Parasu", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "b025ba778ac62937c0ff6c3411b8770d0d3f7cbc", date: "2024-04-25 04:23:25 UTC", description: "Implement `LogEvent::get_mut_by_meaning`", pr_number: 20358, scopes: ["core"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 29, deletions_count: 24},
		{sha: "e4e0ea591cb97ed8308a2aabdde35ec309cf5178", date: "2024-04-25 21:52:18 UTC", description: "Bump down zstd-sys from 2.0.10 to 2.0.9", pr_number: 20369, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 5, insertions_count: 29, deletions_count: 14},
		{sha: "539385408ac20787f00d3847dc8af2b754c3d76a", date: "2024-04-26 22:49:45 UTC", description: "Bump cargo_toml from 0.20.0 to 0.20.2", pr_number: 20375, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "0cf927dccf9cd88e9ffdb8d208e858ff0aef7082", date: "2024-04-27 14:36:31 UTC", description: "fix some typos in comments", pr_number: 20334, scopes: [], type: "chore", breaking_change: false, author: "fuyangpengqi", files_count: 5, insertions_count: 6, deletions_count: 6},
		{sha: "5f08ce8d9a6437243fe7164947aa6f765f3e27e0", date: "2024-04-30 00:30:34 UTC", description: "fix grammar in what-is-observability-pipelines.md", pr_number: 20387, scopes: [], type: "docs", breaking_change: false, author: "Brandon Zylstra", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "bcaba0e22d830dbe058956d833fbf973a6e8d818", date: "2024-04-30 08:37:42 UTC", description: "Install dd-pkg in *-verify workflows and lint in verify-install.sh", pr_number: 20397, scopes: ["ci"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 11, deletions_count: 1},
		{sha: "d463a76365c46d81c8b7bdaaaefa70ae4dfc77d7", date: "2024-04-30 01:14:13 UTC", description: "Bump bufbuild/buf-setup-action from 1.30.1 to 1.31.0", pr_number: 20361, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "dcc6cc82100d238068ae7fb26ff9ecab30f5ccff", date: "2024-04-30 01:14:19 UTC", description: "Bump lapin from 2.3.1 to 2.3.3", pr_number: 20371, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 7},
		{sha: "539333df3256ae360eab1f5d2d28d06170b08db8", date: "2024-04-30 08:17:54 UTC", description: "Bump serde_with from 3.7.0 to 3.8.1", pr_number: 20388, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 15, deletions_count: 15},
		{sha: "f68cacf7d73b867d8aeb8d006e2f3beecc76fc08", date: "2024-04-30 08:18:08 UTC", description: "Bump data-encoding from 2.5.0 to 2.6.0", pr_number: 20389, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "26ece36c9fb9ebd4b189bbf0213ed50d33750d61", date: "2024-04-30 08:18:17 UTC", description: "Bump serde from 1.0.198 to 1.0.199", pr_number: 20390, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "a53251c77d664f7889de218e6838bcc5ef5bc2bb", date: "2024-04-30 08:18:26 UTC", description: "Bump async-compression from 0.4.8 to 0.4.9", pr_number: 20391, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "2346266d0cbd0ea0f748bd73d62bec3bee028497", date: "2024-04-30 08:18:36 UTC", description: "Bump hashbrown from 0.14.3 to 0.14.5", pr_number: 20392, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 16, deletions_count: 16},
		{sha: "3ae189f60f09630e5c1da264e2fd3c7ce69a877b", date: "2024-04-30 08:58:50 UTC", description: "Bump databend-client from 0.17.0 to 0.17.1", pr_number: 20377, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "e6724838e9fb48451b2a799ddbcf3b229ce5a199", date: "2024-04-30 08:59:54 UTC", description: "Bump async-recursion from 1.1.0 to 1.1.1", pr_number: 20376, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "11ace462d514b3e06478a7cb7cfe9694d897701a", date: "2024-04-30 09:00:29 UTC", description: "Bump parking_lot from 0.12.1 to 0.12.2", pr_number: 20378, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "b17b420047031c25834b133e3361c1c03ae5efa6", date: "2024-04-30 02:30:27 UTC", description: "Bump VRL to 0.14.0", pr_number: 20398, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 7, insertions_count: 23, deletions_count: 15},
		{sha: "a5a6c6f99c23484bac6321abe36a56546d314f74", date: "2024-04-30 20:00:32 UTC", description: "support labels", pr_number: 20307, scopes: ["chronicle sink"], type: "enhancement", breaking_change: false, author: "Benjamin Werthmann", files_count: 3, insertions_count: 49, deletions_count: 0},
		{sha: "8a571d12b72503936f070e99e6d4878eb06234f6", date: "2024-04-30 21:33:07 UTC", description: "Bump libc from 0.2.153 to 0.2.154", pr_number: 20402, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "57aed6c9b4f96dcfa7ecca5b25d72cab1ff70e0a", date: "2024-04-30 21:33:33 UTC", description: "Bump socket2 from 0.5.6 to 0.5.7", pr_number: 20401, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 12, deletions_count: 12},
		{sha: "2dc53ff6f862d95274d6932f974c9f9163830449", date: "2024-05-01 04:33:46 UTC", description: "Bump flate2 from 1.0.28 to 1.0.30", pr_number: 20399, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "056c2df60a7f776eca2fad6a4686deee35f9b340", date: "2024-05-01 04:34:14 UTC", description: "Bump cached from 0.49.3 to 0.50.0", pr_number: 20379, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 42},
		{sha: "974c7a93092959fcc3828ac4bbcf04d927380866", date: "2024-05-02 12:46:07 UTC", description: "remove repetitive words", pr_number: 20315, scopes: ["lib", "website"], type: "docs", breaking_change: false, author: "hidewrong", files_count: 3, insertions_count: 3, deletions_count: 3},
		{sha: "61b0b368546b37b2b6f07aa8ad3009caf69d1d39", date: "2024-05-02 02:38:27 UTC", description: "Add future deprecation note for `component_(sent|received)_bytes_total` metrics", pr_number: 20412, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "7f462310b6b74c29d73e26ca75a400f6836f4866", date: "2024-05-02 12:19:17 UTC", description: "nixos.md: Corrected to showcase module usage", pr_number: 20413, scopes: ["platforms"], type: "docs", breaking_change: false, author: "Jonathan Davies", files_count: 1, insertions_count: 35, deletions_count: 7},
		{sha: "51dd03f1ff54ee6810a0842a78fa25b535e8d52f", date: "2024-05-02 05:35:12 UTC", description: "Bump VRL to 0.15.0", pr_number: 20415, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "4472d498b9ba4f10b407284908a7a51f1e70e354", date: "2024-05-02 21:25:58 UTC", description: "Bump vrl from 0.14.0 to 0.15.0", pr_number: 20417, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "73d7caacb7ad8d28177291c49ebd8894eb5b666a", date: "2024-05-02 21:26:12 UTC", description: "Bump serde from 1.0.199 to 1.0.200", pr_number: 20420, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "859996626ff91c15285970266889284c2fe9caaf", date: "2024-05-03 04:27:46 UTC", description: "Bump lapin from 2.3.3 to 2.3.4", pr_number: 20418, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "562bb686a016dc84c1adf7f05592712f09b509f5", date: "2024-05-03 04:28:01 UTC", description: "Bump env_logger from 0.10.2 to 0.11.3", pr_number: 20416, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 21, deletions_count: 11},
		{sha: "b22cea70f8df8d1d2b48f0ddb1733de9e8782cb3", date: "2024-05-03 04:28:26 UTC", description: "Bump rmpv from 1.0.2 to 1.3.0", pr_number: 20410, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "57f602b460af1e7e2c5cd5cb3a7b70bb9db27f75", date: "2024-05-03 04:29:26 UTC", description: "Bump the aws group with 2 updates", pr_number: 20406, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "81861f3b1d9e2ae55c443d57a9b5f53f99d23262", date: "2024-05-03 05:10:45 UTC", description: "Bump governor from 0.6.0 to 0.6.3", pr_number: 20419, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 18, deletions_count: 6},
		{sha: "d549edd95b232aae28f3b663fe128b11b7e9366a", date: "2024-05-03 05:14:10 UTC", description: "Bump base64 from 0.22.0 to 0.22.1", pr_number: 20408, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 12, deletions_count: 11},
		{sha: "fa0b5b388c26fb75f4665621f1962cf304e7fc38", date: "2024-05-03 06:57:08 UTC", description: "Bump roaring from 0.10.3 to 0.10.4", pr_number: 20409, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 14, deletions_count: 3},
		{sha: "ca00cc8835ff70c3c5865aa06925efe9ac25706f", date: "2024-05-03 03:13:05 UTC", description: "Bump rmp-serde from 1.2.0 to 1.3.0", pr_number: 20407, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "8ed9ec24c1eba0b2191d7c1f24ec2a7540b5bebf", date: "2024-05-03 04:39:56 UTC", description: "Remove deprecated `--strict-env-vars` flag", pr_number: 20422, scopes: ["config"], type: "chore", breaking_change: true, author: "Jesse Szwedko", files_count: 16, insertions_count: 107, deletions_count: 223},
		{sha: "381326077ca7fb856c0dd645927ea652c0d70a37", date: "2024-05-03 18:38:56 UTC", description: "Bump timeout for `publish-new-environment` in CI", pr_number: 20426, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "4850a9ae033d474e94c8cec161c8f912a9b7c0d2", date: "2024-05-03 18:41:31 UTC", description: "Regenerate k8s manifests for chart 0.32.1", pr_number: 20280, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "19ba8419eef8f73ff881f382aa655318875bb5ad", date: "2024-05-04 03:53:33 UTC", description: "Support loading secrets from AWS Secrets Manager", pr_number: 20142, scopes: ["config"], type: "feat", breaking_change: false, author: "Tommy Schmidt", files_count: 7, insertions_count: 202, deletions_count: 21},
		{sha: "edcbf4382e383c29c76afc927d511706bb23f040", date: "2024-05-03 22:22:17 UTC", description: "Document new uuid_from_friendly_id function", pr_number: 20357, scopes: [], type: "docs", breaking_change: false, author: "Andrew Martin", files_count: 2, insertions_count: 35, deletions_count: 0},
		{sha: "c2765f45e243636b40d22a819d1496549eb40ed4", date: "2024-05-03 23:18:14 UTC", description: "component features comment trigger one runner label", pr_number: 20430, scopes: ["ci"], type: "fix", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "e5a32a3fbf615a7a31d07eff2b31abea7ef6e560", date: "2024-05-06 19:51:48 UTC", description: "expose internals and test utils", pr_number: 20429, scopes: ["datadog logs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 25, deletions_count: 23},
		{sha: "04a6b55be891a7062ef526779aa0457d5d5b2972", date: "2024-05-06 21:03:51 UTC", description: "Bump timeouts for test-misc and integration tests", pr_number: 20438, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "79a22946c4018d0a07af662ecb4b6fad5d493e45", date: "2024-05-06 19:27:21 UTC", description: "Deprecate the `enterprise` feature", pr_number: 20437, scopes: ["enterprise"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 9, deletions_count: 3},
	]
}
