Fixed a bug where simultaneous updates to Vector configuration and enrichment table files caused
only the enrichment tables to reload. The config watcher now detects Vector config file changes
independently of component-referenced files, so a concurrent config update triggers a full
reload from disk instead of an enrichment-table-only reload.

authors: powerumc
