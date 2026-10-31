Fixed a panic ("Formatting argument out of range") in `vector validate` when the resolved config paths were very long, for example when `-C` or `VECTOR_CONFIG_DIR` contained globs expanding to many directories. The list of config paths in the output is now truncated with `… and N more`.

authors: alexeynl
