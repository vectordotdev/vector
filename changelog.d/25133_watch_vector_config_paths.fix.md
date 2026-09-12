Fixed a bug where simultaneous updates to Vector configuration and enrichment table files caused
only the enrichment tables to reload. The config watcher now detects Vector config file changes
independently of component-referenced files, so a concurrent config update triggers a full
reload from disk instead of an enrichment-table-only reload. When a config file and a
non-enrichment component file (for example a sink TLS certificate) change in the same batch,
Vector reloads the config and force-restarts the affected components.

authors: powerumc
