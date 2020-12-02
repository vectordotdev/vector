package metadata

components: transforms: geoip: {
	title: "GeoIP"

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
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
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		database: {
			description: "Path to the MaxMind GeoIP2 or GeoLite2 binary city database file (`GeoLite2-City.mmdb`). Other databases, such as the the country database are not supported.\n"
			required:    true
			type: string: {
				examples: ["/path/to/GeoLite2-City.mmdb"]
			}
		}
		source: {
			description: "The field name that contains the IP address. This field should contain a valid IPv4 or IPv6 address."
			required:    true
			type: string: {
				examples: ["ip_address", "x-forwarded-for", "parent.child", "array[0]"]
			}
		}
		target: {
			common:      true
			description: "The default field to insert the resulting GeoIP data into. See [output](#output) for more info."
			required:    false
			type: string: {
				default: "geoip"
				examples: ["geoip", "parent.child"]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	output: logs: line: {
		description: "Geo enriched log event"
		fields: {
			geoip: {
				description: "The root field containing all geolocation data as sub-fields."
				required:    true
				type: object: {
					examples: []
					options: {
						city_name: {
							description: "The city name associated with the IP address."
							required:    true
							type: string: {
								examples: ["New York", "Brooklyn", "Chicago"]
							}
						}
						continent_code: {
							description: "The continent code associated with the IP address."
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
							}
						}
						country_code: {
							description: "The [ISO 3166-2 country codes][urls.iso3166-2] associated with the IP address."
							required:    true
							type: string: {
								examples: ["US", "US-PR", "FR", "FR-BL", "GB", "A1", "A2"]
							}
						}
						latitude: {
							description: "The latitude associated with the IP address."
							required:    true
							type: string: {
								examples: ["51.75"]
							}
						}
						longitude: {
							description: "The longitude associated with the IP address."
							required:    true
							type: string: {
								examples: ["-1.25"]
							}
						}
						postal_code: {
							description: "The postal code associated with the IP address."
							required:    true
							type: string: {
								examples: ["07094", "10010", "OX1"]
							}
						}
						timezone: {
							description: "The timezone associated with the IP address in [IANA time zone format][urls.iana_time_zone_format]. A full list of time zones can be found [here][urls.iana_time_zones].\n"
							required:    true
							type: string: {
								examples: ["America/New_York", "Asia/Atyrau", "Europe/London"]
							}
						}
					}
				}
			}
		}
	}
}
