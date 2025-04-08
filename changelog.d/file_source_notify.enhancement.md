The `file` source now supports using filesystem notifications instead of polling for both file watching and file discovery. This significantly reduces CPU and disk I/O usage, especially when monitoring large numbers of files that are mostly idle.

To enable this feature, set `use_notify_for_discovery: true` in your file source configuration.

```toml
[sources.logs]
type = "file"
include = ["/var/log/*.log"]
use_notify_for_discovery = true
```

This implementation:
- Uses OS-level filesystem notifications to detect file changes
- Transitions files between active (polling) and passive (notification-based) states
- Reduces the number of open file handles for idle files
- Improves responsiveness to file changes
- Maintains proper checkpointing for all files

authors: tamer-hassan
