package metadata

remap: functions: parse_etld: {
	category:    "Parse"
	description: """
		Parses the [eTLD](\(urls.etld)) from `value` representing domain name.
		"""

	arguments: [
		{
			name:        "value"
			description: "The domain string."
			required:    true
			type: ["string"]
		},
		{
			name: "plus_parts"
			description: """
				Can be provided to get additional parts of the domain name. When 1 is passed,
				eTLD+1 will be returned, which represents a domain registrable by a single
				organization. Higher numbers will return subdomains.
				"""
			required: false
			type: ["integer"]
			default: false
		},
	]
	internal_failure_reasons: [
		"unable to determine eTLD for `value`",
	]
	return: types: ["object"]

	examples: [
		{
			title: "Parse eTLD"
			source: #"""
				parse_etld!("sub.sussex.ac.uk")
				"""#
			return: {
				etld:         "ac.uk"
				etld_plus:    "ac.uk"
				known_suffix: true
			}
		},
		{
			title: "Parse eTLD+1"
			source: #"""
				parse_etld!("sub.sussex.ac.uk", plus_parts: 1)
				"""#
			return: {
				etld:         "ac.uk"
				etld_plus:    "sussex.ac.uk"
				known_suffix: true
			}
		},
		{
			title: "Parse eTLD with unknown suffix"
			source: #"""
				parse_etld!("vector.acmecorp")
				"""#
			return: {
				etld:         "acmecorp"
				etld_plus:    "acmecorp"
				known_suffix: false
			}
		},
	]
}
