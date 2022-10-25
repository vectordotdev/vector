package metadata

base: components: transforms: geoip: configuration: {
	database: {
		description: """
			Path to the [MaxMind GeoIP2][geoip2] or [GeoLite2 binary city database file][geolite2]
			(**GeoLite2-City.mmdb**).

			Other databases, such as the country database, are not supported.

			[geoip2]: https://dev.maxmind.com/geoip/geoip2/downloadable
			[geolite2]: https://dev.maxmind.com/geoip/geoip2/geolite2/#Download_Access
			"""
		required: true
		type: string: {
			examples: ["/path/to/GeoLite2-City.mmdb", "/path/to/GeoLite2-ISP.mmdb"]
			syntax: "literal"
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
		required: false
		type: string: {
			default: "en"
			examples: ["de", "zh-CN"]
			syntax: "literal"
		}
	}
	source: {
		description: """
			The field name in the event that contains the IP address.

			This field should contain a valid IPv4 or IPv6 address.
			"""
		required: true
		type: string: {
			examples: ["ip_address", "x-forwarded-for", "parent.child", "array[0]"]
			syntax: "literal"
		}
	}
	target: {
		description: """
			The field to insert the resulting GeoIP data into.

			See [output](#output-data) for more info.
			"""
		required: false
		type: string: {
			default: "geoip"
			examples: ["geoip", "parent.child"]
			syntax: "literal"
		}
	}
}
