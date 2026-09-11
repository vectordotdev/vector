Added a new `odbc` source that periodically queries databases through
[ODBC (Open Database Connectivity)](https://en.wikipedia.org/wiki/Open_Database_Connectivity)
and emits each returned row as a structured log event. It supports scheduled and parameterized
queries, batched row fetching, and persisted tracking columns for incremental collection across
runs. A database-specific ODBC driver must be installed separately.

authors: powerumc
