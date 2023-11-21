package metadata

remap: functions: encode_percent: {
	category:    "Codec"
	description: """
		Encodes a `value` with [percent encoding](\(urls.percent_encoded_bytes)) to safely be used in URLs.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to encode."
			required:    true
			type: ["string"]
		},
		{
			name:        "ascii_set"
			description: "The ASCII set to use when encoding the data."
			required:    false
			type: ["string"]
			default: "NON_ALPHANUMERIC"
			enum: {
				NON_ALPHANUMERIC:    "Encode any non-alphanumeric characters. This is the safest option."
				CONTROLS:            "Encode only [control characters](\(urls.percent_encoding_controls))."
				FRAGMENT:            "Encode only [fragment characters](\(urls.percent_encoding_fragment))"
				QUERY:               "Encode only [query characters](\(urls.percent_encoding_query))"
				SPECIAL:             "Encode only [special characters](\(urls.percent_encoding_special))"
				PATH:                "Encode only [path characters](\(urls.percent_encoding_path))"
				USERINFO:            "Encode only [userinfo characters](\(urls.percent_encoding_userinfo))"
				COMPONENT:           "Encode only [component characters](\(urls.percent_encoding_component))"
				WWW_FORM_URLENCODED: "Encode only [`application/x-www-form-urlencoded`](\(urls.percent_encoding_www_form_urlencoded))"
			}
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Percent encode all non-alphanumeric characters (default)"
			source: """
				encode_percent("foo bar?")
				"""
			return: "foo%20bar%3F"
		},
		{
			title: "Percent encode only control characters"
			source: """
				encode_percent("foo \tbar", ascii_set: "CONTROLS")
				"""
			return: "foo %09bar"
		},
	]
}
