package metadata

remap: functions: parse_logfmt: {
	category:    "Parse"
	description: """
		Parses the `value` in [logfmt](\(urls.logfmt)).

		* Keys and values can be wrapped using the `\"` character.
		* `\"` characters can be escaped by the `\\` character.
		* As per this [logfmt specification](\(urls.logfmt_specs)), the `parse_logfmt` function
		  accepts standalone keys and assigns them a Boolean value of `true`.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to parse."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a properly formatted key-value string",
	]
	return: types: ["object"]

	examples: [
		{
			title: "Parse logfmt log"
			source: #"""
				parse_logfmt!(
					"@timestamp=\"Sun Jan 10 16:47:39 EST 2021\" level=info msg=\"Stopping all fetchers\" tag#production=stopping_fetchers id=ConsumerFetcherManager-1382721708341 module=kafka.consumer.ConsumerFetcherManager"
				)
				"""#
			return: {
				"@timestamp":     "Sun Jan 10 16:47:39 EST 2021"
				level:            "info"
				msg:              "Stopping all fetchers"
				"tag#production": "stopping_fetchers"
				id:               "ConsumerFetcherManager-1382721708341"
				module:           "kafka.consumer.ConsumerFetcherManager"
			}
		},
	]
}
