package metadata

remap: functions: parse_dnstap: {
	category: "Parse"
	description: """
		Parses the `value` as base64 encoded DNSTAP data.
		"""
	notices: []

	arguments: [
		{
			name:        "value"
			description: "The base64 encoded representation of the DNSTAP data to parse."
			required:    true
			type: ["string"]
		},
		{
			name: "lowercase_hostnames"
			description: """
				Whether to turn all hostnames found in resulting data lowercase, for consistency.
				"""
			required: false
			default:  false
			type: ["boolean"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid base64 encoded string.",
		"dnstap parsing failed for `value`",
	]
	return: types: ["object"]

	examples: [
		{
			title: "Parse dnstap query message"
			source: #"""
				parse_dnstap!("ChVqYW1lcy1WaXJ0dWFsLU1hY2hpbmUSC0JJTkQgOS4xNi4zGgBy5wEIAxACGAEiEAAAAAAAAAAAAAAAAAAAAAAqECABBQJwlAAAAAAAAAAAADAw8+0CODVA7+zq9wVNMU3WNlI2kwIAAAABAAAAAAABCWZhY2Vib29rMQNjb20AAAEAAQAAKQIAAACAAAAMAAoACOxjCAG9zVgzWgUDY29tAGAAbQAAAAByZLM4AAAAAQAAAAAAAQJoNQdleGFtcGxlA2NvbQAABgABAAApBNABAUAAADkADwA1AAlubyBTRVAgbWF0Y2hpbmcgdGhlIERTIGZvdW5kIGZvciBkbnNzZWMtZmFpbGVkLm9yZy54AQ==")
				"""#
			return: {
				"dataType":      "Message"
				"dataTypeId":    1
				"extraInfo":     ""
				"messageType":   "ResolverQuery"
				"messageTypeId": 3
				"queryZone":     "com."
				"requestData": {
					"fullRcode": 0
					"header": {
						"aa":      false
						"ad":      false
						"anCount": 0
						"arCount": 1
						"cd":      false
						"id":      37634
						"nsCount": 0
						"opcode":  0
						"qdCount": 1
						"qr":      0
						"ra":      false
						"rcode":   0
						"rd":      false
						"tc":      false
					}
					"opt": {
						"do":            true
						"ednsVersion":   0
						"extendedRcode": 0
						"options": [
							{
								"optCode":  10
								"optName":  "Cookie"
								"optValue": "7GMIAb3NWDM="
							},
						]
						"udpPayloadSize": 512
					}
					"question": [
						{
							"class":          "IN"
							"domainName":     "facebook1.com."
							"questionType":   "A"
							"questionTypeId": 1
						},
					]
					"rcodeName": "NoError"
				}
				"responseData": {
					"fullRcode": 16
					"header": {
						"aa":      false
						"ad":      false
						"anCount": 0
						"arCount": 1
						"cd":      false
						"id":      45880
						"nsCount": 0
						"opcode":  0
						"qdCount": 1
						"qr":      0
						"ra":      false
						"rcode":   16
						"rd":      false
						"tc":      false
					}
					"opt": {
						"do":            false
						"ednsVersion":   1
						"extendedRcode": 1
						"ede": [
							{
								"extraText": "no SEP matching the DS found for dnssec-failed.org."
								"infoCode":  9
								"purpose":   "DNSKEY Missing"
							},
						]
						"udpPayloadSize": 1232
					}
					"question": [
						{
							"class":          "IN"
							"domainName":     "h5.example.com."
							"questionType":   "SOA"
							"questionTypeId": 6
						},
					]
					"rcodeName": "BADSIG"
				}
				"responseAddress": "2001:502:7094::30"
				"responsePort":    53
				"serverId":        "james-Virtual-Machine"
				"serverVersion":   "BIND 9.16.3"
				"socketFamily":    "INET6"
				"socketProtocol":  "UDP"
				"sourceAddress":   "::"
				"sourcePort":      46835
				"time":            1_593_489_007_920_014_129
				"timePrecision":   "ns"
				"timestamp":       "2020-06-30T03:50:07.920014129Z"
			}
		},
	]
}
