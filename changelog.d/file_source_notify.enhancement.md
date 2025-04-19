The `file` source has been completely refactored to exclusively use filesystem notifications instead of polling for both file watching and file discovery. This significantly reduces CPU and disk I/O usage, especially when monitoring large numbers of files that are mostly idle.

This implementation:
- Uses OS-level filesystem notifications to detect all file changes
- Eliminates polling completely, using only notification-based watching
- Never keeps file handles open for idle files
- Only opens files when needed for reading, then closes them immediately
- Improves responsiveness to file changes
- Maintains proper checkpointing for all files
- Reads pre-existing files at startup to detect changes that occurred while Vector was stopped
- Filters out empty lines to avoid sending empty events

authors: tamer-hassan
