Fixed a deadlock during config reload that caused all sources to stop consuming
new messages when a sink in wait_for_sinks was being changed. The source pump
would block in wait_for_replacements due to a Pause control message, creating a
circular dependency with shutdown_diff.

authors: joshcoughlan
