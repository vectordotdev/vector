package metadata

services: postgresql: {
	name:     "PostgreSQL"
	thing:    "a \(name) server"
	url:      urls.postgresql
	versions: null

	description: """
		[PostgreSQL](\(urls.postgresql)) (\"Postgres\" for short) is an open source
		relational database management system (RDBMBS).
		"""
}
