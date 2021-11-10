package metadata

components: enrichment_tables: file: {
	title: "File"
	description: '''
			Loads enrichment data from a CSV file.

			For the lookup to be as performant as possible, the data is indexed according to the fields that are used
			in the search. It should be noted that indexes can only be created for fields for which an exact match is
			used in the condition. For range searches, an index is not used and the enrichment table drops back to a
			sequential scan of the data. A sequential scan will not impact performance significantly provided there
			are only a few possible rows returned by the exact matches in the condition. It is not recommended to
			use a condition that only uses date range searches.
		'''

	classes: {
		commonly_used: false
		development:   "beta"
		stateful:      false
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		file: {
			path: {
				description: "The path of the CSV file. Note, Vector needs read only permissions to access this file."
				common:      true
				required:    true
				type: string: {
					examples: [
						"/data/info.csv",
						"./info.csv",
					]
				}
			}
			encoding: {
				type: {
					description: "The encoding of the file. Currently, only CSV is supported."
					required:    true
					type: string: {
						examples: [ "csv"]
					}
				}
				delimiter: {
					description: "The delimiter used to separate fields in each row of the CSV file."
					required:    false
					default:     ","
					type: string: {
						examples: [ ":"]
					}
				}
				include_headers: {
					description: '''
							Set to true if the first row of the CSV file contains the headers for each column.
							If false, there are no headers and the columns are referred to by their numerical
							index.
						'''
					required: false
					default:  true
					type: boolean: {
						examples: [ false]
					}
				}
			}
			schema: {
				common:      true
				description: """
					Key/value pairs representing mapped log field names and types. This is used to
					coerce log fields from strings into their proper types. The available types are
					listed in the **Types** list below.

					Timestamp coercions need to be prefaced with `timestamp|`, for example
					`\"timestamp|%F\"`. Timestamp specifiers can use either of the following:

					1. One of the built-in-formats listed in the **Timestamp Formats** table below.
					2. The [time format specifiers](\(urls.chrono_time_formats)) from Rust's
					`chrono` library.

					### Types

					* `bool`
					* `string`
					* `float`
					* `integer`
					* `date`
					* `timestamp` (see the table below for formats)

					### Timestamp Formats

					Format | Description | Example
					:------|:------------|:-------
					`%F %T` | `YYYY-MM-DD HH:MM:SS` | `2020-12-01 02:37:54`
					`%v %T` | `DD-Mmm-YYYY HH:MM:SS` | `01-Dec-2020 02:37:54`
					`%FT%T` | [ISO 8601](\(urls.iso_8601))\\[RFC 3339](\(urls.rfc_3339)) format without time zone | `2020-12-01T02:37:54`
					`%a, %d %b %Y %T` | [RFC 822](\(urls.rfc_822))/[2822](\(urls.rfc_2822)) without time zone | `Tue, 01 Dec 2020 02:37:54`
					`%a %d %b %T %Y` | [`date`](\(urls.date)) command output without time zone | `Tue 01 Dec 02:37:54 2020`
					`%a %b %e %T %Y` | [ctime](\(urls.ctime)) format | `Tue Dec  1 02:37:54 2020`
					`%s` | [UNIX](\(urls.unix_timestamp)) timestamp | `1606790274`
					`%FT%TZ` | [ISO 8601](\(urls.iso_8601))/[RFC 3339](\(urls.rfc_3339)) UTC | `2020-12-01T09:37:54Z`
					`%+` | [ISO 8601](\(urls.iso_8601))/[RFC 3339](\(urls.rfc_3339)) UTC with time zone | `2020-12-01T02:37:54-07:00`
					`%a %d %b %T %Z %Y` | [`date`](\(urls.date)) command output with time zone | `Tue 01 Dec 02:37:54 PST 2020`
					`%a %d %b %T %z %Y`| [`date`](\(urls.date)) command output with numeric time zone | `Tue 01 Dec 02:37:54 -0700 2020`
					`%a %d %b %T %#z %Y` | [`date`](\(urls.date)) command output with numeric time zone (minutes can be missing or present) | `Tue 01 Dec 02:37:54 -07 2020`

					**Note**: the examples in this table are for 54 seconds after 2:37 am on December 1st, 2020 in Pacific Standard Time.
					"""
				required:    false
				warnings: []

				type: object: {
					examples: [
						{
							status:            "int"
							duration:          "float"
							success:           "bool"
							timestamp_iso8601: "timestamp|%F"
							timestamp_custom:  "timestamp|%a %b %e %T %Y"
							timestamp_unix:    "timestamp|%F %T"
						},
					]
					options: {}
				}
			}
		}
	}
}
