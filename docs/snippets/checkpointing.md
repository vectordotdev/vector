Vector checkpoints the current read position after each successful read. This ensures that Vector resumes where it left off if restarted, preventing data from being read twice. The checkpoint positions are stored in the data directory which is specified via the global [`data_dir`][data_dir] option, but can be overridden via the [`data_dir`](#data_dir) option in the file source directly.

[data_dir]: /docs/reference/configuration/global-options/#data_dir
