package metadata

services: greptimedb: {
	name:     "GreptimeDB"
	thing:    "a \(name) database"
	url:      urls.greptimedb
	versions: null

	description: "[GreptimeDB](\(urls.greptimedb)) is an open-source cloud-native time-series database. It combines time-series and analytic workload into one database, and allows query via both SQL and PromQL. GreptimeDB works seamlessly with modern infrastructure like Kubernetes and object storage. It's also available on [Cloud](\(urls.greptimecloud))."
}
