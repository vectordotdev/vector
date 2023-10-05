package metadata

remap: functions: encode_logfmt: {
	category:    "Codec"
	description: """
		Encodes the `value` to [logfmt](\(urls.logfmt)).
		"""
	notices:     functions.encode_key_value.notices

	arguments: [
		{
			name:        "value"
			description: "The value to convert to a logfmt string."
			required:    true
			type: ["object"]
		},
		{
			name:        "fields_ordering"
			description: "The ordering of fields to preserve. Any fields not in this list are listed unordered, after all ordered fields."
			required:    false
			type: ["array"]
		},
	]
	internal_failure_reasons: [
		"`fields_ordering` contains a non-string element.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Encode to logfmt (no ordering)"
			source: """
				encode_logfmt({"ts": "2021-06-05T17:20:00Z", "msg": "This is a message", "lvl": "info"})
				"""
			return: #"lvl=info msg="This is a message" ts=2021-06-05T17:20:00Z"#
		},
		{
			title: "Encode to logfmt (fields ordering)"
			source: """
				encode_logfmt!({"ts": "2021-06-05T17:20:00Z", "msg": "This is a message", "lvl": "info", "log_id": 12345}, ["ts", "lvl", "msg"])
				"""
			return: #"ts=2021-06-05T17:20:00Z lvl=info msg="This is a message" log_id=12345"#
		},
		{
			title: "Encode to logfmt (nested fields)"
			source: """
				encode_logfmt({"agent": {"name": "foo"}, "log": {"file": {"path": "my.log"}}, "event": "log"})
				"""
			return: #"agent.name=foo event=log log.file.path=my.log"#
		},
		{
			title: "Encode to logfmt (nested fields ordering)"
			source: """
				encode_logfmt!({"agent": {"name": "foo"}, "log": {"file": {"path": "my.log"}}, "event": "log"}, ["event", "log.file.path", "agent.name"])
				"""
			return: #"event=log log.file.path=my.log agent.name=foo"#
		},
	]
}
