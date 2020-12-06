package metadata

services: syslog: {
	name:     "Syslog"
	thing:    name
	url:      urls.syslog
	versions: null

	description: "[Syslog](\(urls.syslog)) stands for System Logging Protocol and is a standard protocol used to send system log or event messages to a specific server, called a syslog server. It is used to collect various device logs from different machines and send them to a central location for monitoring and review."
}
