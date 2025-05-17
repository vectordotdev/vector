A new `ifile` source has been added as an improved version of the `file` source. The `ifile` source is a complete rewrite that addresses several limitations of the original implementation while maintaining compatibility with existing configurations.

Key improvements in the `ifile` source:

- **Fully async implementation**: Uses async/await throughout for better performance and resource utilization
- **Cross-platform filesystem notifications**: Uses the notify-rs library to detect file changes through OS-level notifications instead of polling
- **Improved file discovery**: Detects new files within milliseconds using filesystem notifications instead of periodic globbing. Only globs once on startup for initial discovery, and therefore obsoletes and removes the `glob_minimum_cooldown_ms` option
- **Resource efficiency**: Never keeps file handles open for idle files, only opening them when needed for reading
- **Better shutdown behavior**: Properly handles shutdown signals and gracefully closes all resources
- **Improved checkpointing**: Introduces a new `checkpoint_interval` configuration option
- **Startup consistency**: Reads files at startup to detect changes that occurred while Vector was stopped
- **Intelligent throttling**: Optimizes CPU usage and log verbosity with smart event handling
- **Better error handling**: Properly handles file deletion events to prevent repeated error messages

The original `file` source remains unchanged and fully supported. Users can migrate to the `ifile` source at their convenience to take advantage of these improvements.

authors: tamer-hassan
