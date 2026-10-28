Fixed a panic ("Formatting argument out of range") in `vector validate` when the resolved config paths were very long, for example when `-C` or `VECTOR_CONFIG_DIR` contained globs expanding to many directories. The number of config paths is now printed, followed by the paths one per line.

authors: alexeynl
