A new configuration option include_paths_glob_patterns has been introduced in the Kubernetes Logs source. This option works alongside the existing exclude_paths_glob_patterns to help narrow down the selection of logs to be considered. include_paths_glob_patterns is evaluated before exclude_paths_glob_patterns.

authors: syedriko
