package metadata

services: postgres: {
	name:     "Postgres"
	thing:    "a \(name) database"
	url:      urls.postgresql
	versions: null

	description: "[PostgreSQL](\(urls.postgresql)) is a powerful, open source object-relational database system that uses and extends the SQL language combined with many features that safely store and scale the most complicated data workloads."
}
