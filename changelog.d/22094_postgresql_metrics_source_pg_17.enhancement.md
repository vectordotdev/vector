Updates the `postgresql_metrics` source to support PostgreSQL 17.

When upgrading to PostgreSQL 17, some metrics will effectively be renamed due to having been moved to
another view. However, these metrics have, until now, not been collected on PostgreSQL 17+
versions, so there is no actual breakage.

authors: biggerfisch bahildebrand
