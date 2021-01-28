package metadata

components: transforms: geoip: {
	title: "GeoIP"

	description: """
		Enrich events with geolocation data from the MaxMind GeoIP2-City,
		GeoLite2-City, GeoIP2-ISP and GeoLite2-ASN databases.
		"""

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		enrich: {
			from: service: {
				name:     "MaxMind GeoIP2 and GeoLite2 city databases"
				url:      urls.maxmind_geoip2_isp
				versions: ">= 2"
			}
		}
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
		database: {
			description: """
				Path to the [MaxMind GeoIP2](\(urls.maxmind_geoip2)) or [GeoLite2 binary city
				database](\(urls.maxmind_geolite2_city)) file (`GeoLite2-City.mmdb`). Other
				databases, such as the the country database, are not supported.
				"""
			required:    true
			type: string: {
				examples: ["/path/to/GeoLite2-City.mmdb", "/path/to/GeoLite2-ISP.mmdb"]
				syntax: "literal"
			}
		}
		source: {
			description: "The field name that contains the IP address. This field should contain a valid IPv4 or IPv6 address."
			required:    true
			type: string: {
				examples: ["ip_address", "x-forwarded-for", "parent.child", "array[0]"]
				syntax: "literal"
			}
		}
		target: {
			common:      true
			description: "The default field to insert the resulting GeoIP data into. See [output](#output) for more info."
			required:    false
			type: string: {
				default: "geoip"
				examples: ["geoip", "parent.child"]
				syntax: "literal"
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	how_it_works: {
		supported_databases: {
			title: "Supported MaxMind databases"
			body:  """
				The `geoip` transform currently supports the following [MaxMind](\(urls.maxmind))
				databases:

				* [GeoLite2-ASN.mmdb](\(urls.maxmind_geolite2_asn)) (free) — Determine the
					autonomous system number and organization associated with an IP address.
				* [GeoLite2-City.mmdb](\(urls.maxmind_geolite2_city)) (free) — Determine the
					country, subdivisions, city, and postal code associated with IPv4 and IPv6
					addresses worldwide.
				* [GeoIP2-City.mmdb](\(urls.maxmind_geoip2_city)) (paid) — Determine the country,
					subdivisions, city, and postal code associated with IPv4 and IPv6
					addresses worldwide.
				* [GeoIP2-ISP.mmdb](\(urls.maxmind_geoip2_isp)) (paid) — Determine the Internet
					Service Provider (ISP), organization name, and autonomous system organization
					and number associated with an IP address.

				The database files should be in the [MaxMind DB file
				format](\(urls.maxmind_db_file_format)).
				"""
		}
	}

	output: logs: line: {
		_city_db_blurb: """
			Available with the [GeoIP2-City](\(urls.maxmind_geoip2_city)) or
			[GeoLite2-City](\(urls.maxmind_geolite2_city)) database.
			"""

		description: "Geo-enriched log event"
		fields: {
			geoip: {
				description: """
					The root field containing all geolocation data as subfields. Depending on the
					database used, either the city or the ISP field is populated.
					"""
				required: true
				type: object: {
					examples: []
					options: {
						autonomous_system_number: {
							description: """
								The Autonomous System (AS) number associated with the IP address.
								Zero if unknown. Available with the
								[GeoIP2-ISP](\(urls.maxmind_geoip2_isp)) or
								[GeoLite2-ASN](\(urls.maxmind_geolite2_asn)) database.
								"""
							required:    false
							common:      false
							type: uint: {
								unit:    null
								default: null
								examples: [701, 721]
							}
							groups: ["ASN", "ISP"]
						}
						autonomous_system_organization: {
							description: """
							The organization associated with the registered autonomous system number
							for the IP address. Available with the
							[GeoIP2-ISP](\(urls.maxmind_geoip2_isp)) or
							[GeoLite2-ASN](\(urls.maxmind_geolite2_asn)) database.
							"""
							required:    false
							common:      false
							type: string: {
								default: null
								examples: [
									"MCI Communications Services, Inc. d/b/a Verizon Business",
									"DoD Network Information Center",
								]
								syntax: "literal"
							}
							groups: ["ASN", "ISP"]
						}
						city_name: {
							description: """
								The city name associated with the IP address. \(_city_db_blurb).
								"""
							required:    true
							type: string: {
								examples: ["New York", "Brooklyn", "Chicago"]
								syntax: "literal"
							}
							groups: ["City"]
						}
						continent_code: {
							description: """
								The continent code associated with the IP address.
								\(_city_db_blurb).
								"""
							required:    true
							type: string: {
								enum: {
									AF: "Africa"
									AN: "Antarctica"
									AS: "Asia"
									EU: "Europe"
									NA: "North America"
									OC: "Oceania"
									SA: "South America"
								}
								syntax: "literal"
							}
							groups: ["City"]
						}
						country_code: {
							description: """
								The [ISO 3166-2 country codes](\(urls.iso3166_2)) associated with
								the IP address. \(_city_db_blurb).
								"""
							required:    true
							type: string: {
								examples: ["US", "US-PR", "FR", "FR-BL", "GB", "A1", "A2"]
								syntax: "literal"
							}
							groups: ["City"]
						}
						isp: {
							description: """
								The name of the Internet Service Provider (ISP) associated with the
								IP address. Available with the
								[GeoIP2-ISP](\(urls.maxmind_geoip2_isp)) database.
								"""
							required:    false
							common:      false
							type: string: {
								default: null
								examples: ["Verizon Business"]
								syntax: "literal"
							}
							groups: ["ISP"]
						}
						latitude: {
							description: "The latitude associated with the IP address. \(_city_db_blurb)."
							required:    true
							type: string: {
								examples: ["51.75"]
								syntax: "literal"
							}
							groups: ["City"]
						}
						longitude: {
							description: "The longitude associated with the IP address. \(_city_db_blurb)."
							required:    true
							type: string: {
								examples: ["-1.25"]
								syntax: "literal"
							}
							groups: ["City"]
						}
						organization: {
							description: """
								The name of the organization associated with the IP address.
								Available with the [GeoIP2-ISP](\(urls.maxmind_geoip2_isp))
								database.
								"""
							required:    false
							common:      false
							type: string: {
								default: null
								examples: ["Verizon Business"]
								syntax: "literal"
							}
							groups: ["ISP"]
						}
						postal_code: {
							description: """
								The postal code associated with the IP address. \(_city_db_blurb).
								"""
							required:    true
							type: string: {
								examples: ["07094", "10010", "OX1"]
								syntax: "literal"
							}
							groups: ["City"]
						}
						timezone: {
							description: """
								The timezone associated with the IP address in [IANA time zone
								format](\(urls.iana_time_zone_format)). A full list of time zones
								can be found [here](\(urls.iana_time_zones)) \(_city_db_blurb).
								"""
							required:    true
							type: string: {
								examples: ["America/New_York", "Asia/Atyrau", "Europe/London"]
								syntax: "literal"
							}
							groups: ["City"]
						}
					}
				}
			}
		}
	}

	telemetry: metrics: {
		processing_errors_total: components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
