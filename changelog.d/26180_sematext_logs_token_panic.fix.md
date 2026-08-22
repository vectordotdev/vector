The `sematext_logs` sink no longer panics when the `token` cannot be parsed as a template (for example `{{ }}`); it now fails configuration validation with a clear error instead of crashing at startup.

authors: thomasqueirozb
