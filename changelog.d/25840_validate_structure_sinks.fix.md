`vector validate` now reports structural errors in the `clickhouse` sink configuration, such as
template confinement violations, invalid batch settings, and conflicting authentication, instead of
accepting the configuration and failing at boot. Sink construction is split into phases so these
checks run once, without reaching the network.

The remaining sinks will be migrated in follow-up changes.

authors: thomasqueirozb
