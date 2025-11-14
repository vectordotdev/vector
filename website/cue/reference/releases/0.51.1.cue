package metadata

releases: "0.51.1": {
	date:     "2025-11-13"
	codename: ""

	whats_next: []

	description: """
		* When Vector is running with debug logs enabled (`VECTOR_LOG=debug`), threads no
		longer panic when logging utilization or other debug messages.

		* The `config_reload_rejected` and `config_reloaded` counters added in `0.51.0` were
		not being emitted and have been replaced. `component_errors_total` and
		`error_code="reload"` replaces `config_reload_rejected` and `reloaded_total`
		replaces `config_reloaded`.

		* The `basename`, `dirname` and `split_path` VRL functions added in `0.51.0` are now
		properly exposed.

		* `blackhole` sink's periodic statistics messages are no longer rate limited.

		* The `internal_logs` source now captures all internal Vector logs without rate limiting.
		Previously, repeated log messages were silently dropped.
		"""

	changelog: [
		{
			type: "fix"
			description: """
				The `blackhole` sink's periodic statistics messages (controlled by `print_interval_secs`) are no longer incorrectly suppressed by rate limiting. These informational messages now appear at the user-configured interval as expected.
				"""
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: """
				Fixed a panic in the tracing rate limiter when a config reload failed. While the panic didn't kill Vector (it was caught by tokio's task
				runtime), it could cause unexpected behavior. The rate limiter now gracefully handles events without standard message fields.
				"""
			contributors: ["pront"]
		},
		{
			type: "enhancement"
			description: """
				The `component_errors_total` metric now includes a `reason` tag when `error_code="reload"` to provide more granular information about reload
				failures. Possible reasons include:

				- `global_options_changed`: Reload rejected because global options (like `data_dir`) changed
				- `global_diff_failed`: Reload rejected because computing global config diff failed
				- `topology_build_failed`: Reload rejected because new topology failed to build/healthcheck
				- `restore_failed`: Reload failed and could not restore previous config

				Replaced metrics:

				- `config_reload_rejected` was replaced by `component_errors_total` with `error_code="reload"` and a `reason` tag specifying the rejection type
				- `config_reloaded` was replaced by the existing `reloaded_total` metric

				Note: The replaced metrics were introduced in v0.50.0 but were never emitted due to a bug. These changes provide consistency across Vector's internal telemetry.
				"""
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: """
				The `internal_logs` source now captures all internal Vector logs without rate limiting. Previously, repeated log messages were silently
				dropped.
				"""
			contributors: ["pront"]
		},
	]

	vrl_changelog: """
		### [0.28.1 (2025-11-07)]

		#### Fixes

		- Fixed an issue where `split_path`, `basename`, `dirname` had not been added to VRL's standard
		library and, therefore, appeared to be missing and were inaccessible in the `0.28.0` release.

		authors: thomasqueirozb (https://github.com/vectordotdev/vrl/pull/1553)


		### [0.28.0 (2025-11-03)]
		"""

	commits: [
		{sha: "0aedea9561a4834f6abebaa2a0bc5580b9143a9e", date: "2025-11-04 02:03:46 UTC", description: "reorganize integration test files", pr_number: 24108, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 125, insertions_count: 149, deletions_count: 122},
		{sha: "2d3793e96d7047408d6ce24d378d2396ca6830f4", date: "2025-11-05 01:13:53 UTC", description: "move all utils in a new utils folder", pr_number: 24143, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 40, insertions_count: 242, deletions_count: 174},
		{sha: "35a408804d9c4453852ff357c15d7ab3aaad5cbd", date: "2025-11-05 20:54:56 UTC", description: "improve/fix minor release template", pr_number: 24156, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 23, deletions_count: 27},
		{sha: "0cce521b4a2eb2a92cb024e46e7c6ffcb1c64754", date: "2025-11-06 02:17:02 UTC", description: "make modules visible to rustfmt", pr_number: 24162, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 19, insertions_count: 218, deletions_count: 117},
		{sha: "4add1c3aa9ebe05d2e16a56afc3ee8accf7cfeb1", date: "2025-11-06 03:28:00 UTC", description: "release prepare vrl version pinning", pr_number: 24158, scopes: ["vdev"], type: "fix", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 66, deletions_count: 33},
		{sha: "1e8e99a4898958ae9b56ee33af162c98092ed9b9", date: "2025-11-06 21:29:00 UTC", description: "update VRL to add missing stdlib fns from 0.28", pr_number: 24178, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "fbbcffc1414041be01bff8963727ceedb2f7fe70", date: "2025-11-07 21:03:54 UTC", description: "temporarily remove homebrew publish step from publish workflow", pr_number: 24185, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 0, deletions_count: 12},
		{sha: "c855585c3386324b26ad2b8516c16177bc860d20", date: "2025-11-07 23:52:13 UTC", description: "disable rate limiting for periodic stats messages", pr_number: 24190, scopes: ["blackhole sink"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 5, deletions_count: 0},
		{sha: "a22b790bcf2e111e7c1b6ffc2de1394fe37b7ae2", date: "2025-11-10 20:17:52 UTC", description: "Disable rate limiting for critical internal error logs", pr_number: 24192, scopes: ["internal logs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 6, insertions_count: 42, deletions_count: 14},
		{sha: "87e7cb8733a6e5b0afe075e54be3cc397023c128", date: "2025-11-10 23:10:54 UTC", description: "prevent panic for traces without standard fields", pr_number: 24191, scopes: ["tracing"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 56, deletions_count: 8},
		{sha: "a7e68b17010c58d0ac2a1656fe13468063c6ddf3", date: "2025-11-11 00:18:14 UTC", description: "do not rate limit utlization report", pr_number: 24202, scopes: ["tracing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 15},
		{sha: "7d657023131d15334255a29a390f2a3604ff67cc", date: "2025-11-11 20:01:07 UTC", description: "move config_reload_* metrics to VectorReload*", pr_number: 24203, scopes: ["internal metrics"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 11, insertions_count: 184, deletions_count: 265},
		{sha: "5432632b414472e004cc68a0cae56f0b4451e8af", date: "2025-11-11 23:42:09 UTC", description: "Add 0.51.0 known issues", pr_number: 24211, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 27, deletions_count: 5},
		{sha: "3e09dc86b8aaf6606abc9c5acc9ddf52f39f6e17", date: "2025-11-12 04:31:25 UTC", description: "prepare v0.51.1 release", pr_number: 24214, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Thomas", files_count: 204, insertions_count: 1055, deletions_count: 756},
		{sha: "3ca3ec6f55522c406680a183416bf7a8b35372ae", date: "2025-11-13 02:07:06 UTC", description: "remove rate limit", pr_number: 24218, scopes: ["internal_logs source"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 48, deletions_count: 3},
	]
}
