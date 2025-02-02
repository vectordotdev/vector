package metadata

base: components: global_options: enrichment_tables: configuration: {
	file: {
		description:   "File-specific settings."
		relevant_when: "type = \"file\""
		required:      true
		type: object: options: {
			encoding: {
				description: "File encoding configuration."
				required:    true
				type: object: options: {
					delimiter: {
						description: "The delimiter used to separate fields in each row of the CSV file."
						required:    false
						type: string: default: ","
					}
					include_headers: {
						description: """
																Whether or not the file contains column headers.

																When set to `true`, the first row of the CSV file will be read as the header row, and
																the values will be used for the names of each column. This is the default behavior.

																When set to `false`, columns are referred to by their numerical index.
																"""
						required: false
						type: bool: default: true
					}
					type: {
						description: "File encoding type."
						required:    true
						type: string: enum: csv: """
																			Decodes the file as a [CSV][csv] (comma-separated values) file.

																			[csv]: https://wikipedia.org/wiki/Comma-separated_values
																			"""
					}
				}
			}
			path: {
				description: """
					The path of the enrichment table file.

					Currently, only [CSV][csv] files are supported.

					[csv]: https://en.wikipedia.org/wiki/Comma-separated_values
					"""
				required: true
				type: string: {}
			}
		}
	}
	flush_interval: {
		description: """
			The interval used for making writes visible in the table.
			Longer intervals might get better performance,
			but there is a longer delay before the data is visible in the table.
			Since every TTL scan makes its changes visible, only use this value
			if it is shorter than the `scan_interval`.

			By default, all writes are made visible immediately.
			"""
		relevant_when: "type = \"memory\""
		required:      false
		type: uint: {}
	}
	internal_metrics: {
		description:   "Configuration of internal metrics"
		relevant_when: "type = \"memory\""
		required:      false
		type: object: options: include_key_tag: {
			description: """
				Determines whether to include the key tag on internal metrics.

				This is useful for distinguishing between different keys while monitoring. However, the tag's
				cardinality is unbounded.
				"""
			required: false
			type: bool: default: false
		}
	}
	locale: {
		description: """
			The locale to use when querying the database.

			MaxMind includes localized versions of some of the fields within their database, such as
			country name. This setting can control which of those localized versions are returned by the
			transform.

			More information on which portions of the geolocation data are localized, and what languages
			are available, can be found [here][locale_docs].

			[locale_docs]: https://support.maxmind.com/hc/en-us/articles/4414877149467-IP-Geolocation-Data#h_01FRRGRYTGZB29ERDBZCX3MR8Q
			"""
		relevant_when: "type = \"geoip\""
		required:      false
		type: string: default: "en"
	}
	max_byte_size: {
		description: """
			Maximum size of the table in bytes. All insertions that make
			this table bigger than the maximum size are rejected.

			By default, there is no size limit.
			"""
		relevant_when: "type = \"memory\""
		required:      false
		type: uint: {}
	}
	path: {
		description: """
			Path to the [MaxMind GeoIP2][geoip2] or [GeoLite2 binary city database file][geolite2]
			(**GeoLite2-City.mmdb**).

			Other databases, such as the country database, are not supported.
			`mmdb` enrichment table can be used for other databases.

			[geoip2]: https://dev.maxmind.com/geoip/geoip2/downloadable
			[geolite2]: https://dev.maxmind.com/geoip/geoip2/geolite2/#Download_Access
			"""
		relevant_when: "type = \"geoip\" or type = \"mmdb\""
		required:      true
		type: string: {}
	}
	scan_interval: {
		description: """
			The scan interval used to look for expired records. This is provided
			as an optimization to ensure that TTL is updated, but without doing
			too many cache scans.
			"""
		relevant_when: "type = \"memory\""
		required:      false
		type: uint: default: 30
	}
	schema: {
		description: """
			Key/value pairs representing mapped log field names and types.

			This is used to coerce log fields from strings into their proper types. The available types are listed in the `Types` list below.

			Timestamp coercions need to be prefaced with `timestamp|`, for example `"timestamp|%F"`. Timestamp specifiers can use either of the following:

			1. One of the built-in-formats listed in the `Timestamp Formats` table below.
			2. The [time format specifiers][chrono_fmt] from Rust’s `chrono` library.

			### Types

			- **`bool`**
			- **`string`**
			- **`float`**
			- **`integer`**
			- **`date`**
			- **`timestamp`** (see the table below for formats)

			### Timestamp Formats

			| Format               | Description                                                                      | Example                          |
			|----------------------|----------------------------------------------------------------------------------|----------------------------------|
			| `%F %T`              | `YYYY-MM-DD HH:MM:SS`                                                            | `2020-12-01 02:37:54`            |
			| `%v %T`              | `DD-Mmm-YYYY HH:MM:SS`                                                           | `01-Dec-2020 02:37:54`           |
			| `%FT%T`              | [ISO 8601][iso8601]/[RFC 3339][rfc3339], without time zone                       | `2020-12-01T02:37:54`            |
			| `%FT%TZ`             | [ISO 8601][iso8601]/[RFC 3339][rfc3339], UTC                                     | `2020-12-01T09:37:54Z`           |
			| `%+`                 | [ISO 8601][iso8601]/[RFC 3339][rfc3339], UTC, with time zone                     | `2020-12-01T02:37:54-07:00`      |
			| `%a, %d %b %Y %T`    | [RFC 822][rfc822]/[RFC 2822][rfc2822], without time zone                         | `Tue, 01 Dec 2020 02:37:54`      |
			| `%a %b %e %T %Y`     | [ctime][ctime] format                                                            | `Tue Dec 1 02:37:54 2020`        |
			| `%s`                 | [UNIX timestamp][unix_ts]                                                        | `1606790274`                     |
			| `%a %d %b %T %Y`     | [date][date] command, without time zone                                          | `Tue 01 Dec 02:37:54 2020`       |
			| `%a %d %b %T %Z %Y`  | [date][date] command, with time zone                                             | `Tue 01 Dec 02:37:54 PST 2020`   |
			| `%a %d %b %T %z %Y`  | [date][date] command, with numeric time zone                                     | `Tue 01 Dec 02:37:54 -0700 2020` |
			| `%a %d %b %T %#z %Y` | [date][date] command, with numeric time zone (minutes can be missing or present) | `Tue 01 Dec 02:37:54 -07 2020`   |

			[date]: https://man7.org/linux/man-pages/man1/date.1.html
			[ctime]: https://www.cplusplus.com/reference/ctime
			[unix_ts]: https://en.wikipedia.org/wiki/Unix_time
			[rfc822]: https://tools.ietf.org/html/rfc822#section-5
			[rfc2822]: https://tools.ietf.org/html/rfc2822#section-3.3
			[iso8601]: https://en.wikipedia.org/wiki/ISO_8601
			[rfc3339]: https://tools.ietf.org/html/rfc3339
			[chrono_fmt]: https://docs.rs/chrono/latest/chrono/format/strftime/index.html#specifiers
			"""
		relevant_when: "type = \"file\""
		required:      false
		type: object: options: "*": {
			description: "represent mapped log field names and types."
			required:    true
			type: string: {}
		}
	}
	ttl: {
		description: """
			TTL (time-to-live in seconds) is used to limit the lifetime of data stored in the cache.
			When TTL expires, data behind a specific key in the cache is removed.
			TTL is reset when the key is replaced.
			"""
		relevant_when: "type = \"memory\""
		required:      false
		type: uint: default: 600
	}
	type: {
		description: "enrichment table type"
		required:    true
		type: string: enum: {
			file: "Exposes data from a static file as an enrichment table."
			geoip: """
				Exposes data from a [MaxMind][maxmind] [GeoIP2][geoip2] database as an enrichment table.

				[maxmind]: https://www.maxmind.com/
				[geoip2]: https://www.maxmind.com/en/geoip2-databases
				"""
			memory: """
				Exposes data from a memory cache as an enrichment table. The cache can be written to using
				a sink.
				"""
			mmdb: """
				Exposes data from a [MaxMind][maxmind] database as an enrichment table.

				[maxmind]: https://www.maxmind.com/
				"""
		}
	}
}
