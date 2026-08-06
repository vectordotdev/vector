package metadata

services: clickhouse: {
	name:     "ClickHouse"
	thing:    "a \(name) database"
	url:      urls.clickhouse
	versions: null

	description: "[ClickHouse](\(urls.clickhouse)) is an open-source column-oriented database management system that manages extremely large volumes of data, including non-aggregated data, in a stable and sustainable manner and allows generating custom data reports in real time. The system is linearly scalable and can be scaled up to store and process trillions of rows and petabytes of data. This makes it an best-in-class storage for logs and metrics data."
}
