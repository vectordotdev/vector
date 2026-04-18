package metadata

services: ydb: {
	name:     "YDB"
	thing:    "a \(name) database"
	url:      urls.ydb
	versions: null

	description: "[YDB](\(urls.ydb)) (Yandex Database) is an open-source Distributed SQL Database that combines high availability and scalability with strong consistency and ACID transactions. It is a versatile database for OLTP and OLAP workloads with built-in fault tolerance."
}
