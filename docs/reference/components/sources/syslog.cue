package metadata

components: sources: syslog: {
	title:             "Syslog"
	short_description: "Ingests data through the [Syslog protocol][urls.syslog_5424] and outputs log events."
	long_description:  "[Syslog][urls.syslog] stands for System Logging Protocol and is a standard protocol used to send system log or event messages to a specific server, called a syslog server. It is used to collect various device logs from different machines and send them to a central location for monitoring and review."

	classes: sources.socket.classes
	features: sources.socket.features
	statuses: sources.socket.statuses
	support: sources.socket.support
	configuration: sources.socket.configuration

	output: logs: line: {
		description: "An individual event from Syslog."
		fields: {
			host:      fields._local_host
			message:   fields._raw_line
			timestamp: fields._current_timestamp
		}
	}

	examples: log: [
		{
			_line: #"""
				2019-02-13T19:48:34+00:00 [info] Started GET "/" for 127.0.0.1
				"""#
			title: "Syslog line"
			configuration: {}
			input: """
				```text
				\( _line )
				```
				"""
			output: {
				timestamp: _values.current_timestamp
				message:   _line
				host:      _values.local_host
			}
		}]
}
