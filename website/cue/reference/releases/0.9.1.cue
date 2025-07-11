package metadata

releases: "0.9.1": {
	date: "2020-04-29"

	whats_next: []

	commits: [
		{sha: "4d76e751febd778887a7432263f77369895cd093", date: "2020-04-22 14:37:44 +0000", description: "Support millisecond and nanosecond timestamps", pr_number: 2382, scopes: ["splunk_hec source", "splunk_hec sink"], type: "fix", breaking_change: false, author: "Lucio Franco", files_count: 2, insertions_count: 71, deletions_count: 4},
		{sha: "b1c8421357502e1eca123e98787e7071109620f4", date: "2020-04-22 15:13:54 +0000", description: "Handle missing source timestamp", pr_number: 2387, scopes: ["journald source"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 30, deletions_count: 4},
		{sha: "fc2c77b643e02b86a99cff4c914df06060a49d52", date: "2020-04-23 15:27:26 +0000", description: "`enoding.only_fields` should properly handle parent keys", pr_number: 2413, scopes: ["config"], type: "fix", breaking_change: false, author: "Ana Hobden", files_count: 3, insertions_count: 110, deletions_count: 49},
		{sha: "48a6d142e9a8ff441d3379cecba7272152b74a72", date: "2020-04-27 13:31:51 +0000", description: "add text encoding", pr_number: 2468, scopes: ["humio_logs sink"], type: "enhancement", breaking_change: false, author: "Luke Steensen", files_count: 4, insertions_count: 28, deletions_count: 74},
		{sha: "47bf9f74903162a02f40cd7113c37cfec6bb4303", date: "2020-04-27 20:05:10 +0000", description: "Use header auth", pr_number: 2443, scopes: ["datadog_metrics sink"], type: "fix", breaking_change: false, author: "Lucio Franco", files_count: 1, insertions_count: 7, deletions_count: 3},
		{sha: "332e9711c7a7c414a0ee83257d172c9b79f1244b", date: "2020-04-28 19:08:02 +0000", description: "Add indexed fields in `text` encoding", pr_number: 2448, scopes: ["splunk_hec sink"], type: "fix", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 1, insertions_count: 67, deletions_count: 22},
		{sha: "fafdc789b26e23075aa6afc1b12622b001f0f5c4", date: "2020-04-28 14:04:02 +0000", description: "Treat empty namespaces as not set", pr_number: 2479, scopes: ["aws_ec2_metadata transform"], type: "fix", breaking_change: false, author: "Lucio Franco", files_count: 1, insertions_count: 48, deletions_count: 8},
		{sha: "4e55cbb5b4879bcd971787089873cd62f7ddc451", date: "2020-04-29 11:30:48 +0000", description: "Fix handling of standard AWS regions", pr_number: 2489, scopes: ["elasticsearch sink"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 98, deletions_count: 31},
		{sha: "a8fba10bc739fb5f9b54264bab937700e161f5d5", date: "2020-04-29 13:42:40 +0000", description: "Fetch system ca certs via schannel on windows", pr_number: 2444, scopes: ["networking"], type: "fix", breaking_change: false, author: "Lucio Franco", files_count: 5, insertions_count: 163, deletions_count: 11},
		{sha: "9082b1808115bac6dd64f310126fb57754ce315e", date: "2020-04-29 15:29:34 +0000", description: "Move healtcheck consumer creation to boxed future", pr_number: 2499, scopes: ["pulsar sink"], type: "fix", breaking_change: false, author: "Evan Cameron", files_count: 1, insertions_count: 13, deletions_count: 17},
		{sha: "b2bc1b77ac53b412162a845293487586f66b3007", date: "2020-04-29 22:26:19 +0000", description: "Add `instance-type` field", pr_number: 2500, scopes: ["aws_ec2_metadata transform"], type: "enhancement", breaking_change: false, author: "Slawomir Skowron", files_count: 1, insertions_count: 16, deletions_count: 0},
		{sha: "af544f361cc03e31207fcdd5e57104d051fde136", date: "2020-04-30 10:51:02 +0000", description: "Use specific error for x509 from system ca", pr_number: 2507, scopes: ["security"], type: "fix", breaking_change: false, author: "Lucio Franco", files_count: 2, insertions_count: 10, deletions_count: 5},
		{sha: "a0d5cf5469045d066bed5ed950187ff6a7612dc4", date: "2020-04-30 12:55:08 +0000", description: "Shutdown topology pieces before building new ones", pr_number: 2449, scopes: ["config"], type: "fix", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 6, insertions_count: 349, deletions_count: 114},
		{sha: "319a75ddc20060a8aecb2d0e990d3e52b19cc0e5", date: "2020-04-30 13:28:53 +0000", description: "Enforce age requirements", pr_number: 2437, scopes: ["aws_cloudwatch_logs sink"], type: "enhancement", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 2, insertions_count: 195, deletions_count: 34},
		{sha: "fcd5c1893713e08d1ee0f51cdca5aa16686af148", date: "2020-04-30 11:17:14 +0000", description: "Check code on Windows", pr_number: 2506, scopes: ["operations"], type: "chore", breaking_change: false, author: "Binary Logic", files_count: 3, insertions_count: 640, deletions_count: 605},
	]
}
