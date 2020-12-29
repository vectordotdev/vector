package metadata

services: journald: {
	name:     "JournalD"
	thing:    name
	url:      urls.journald
	versions: null

	description: "[Journald](\(urls.journald)) is a utility for accessing log data across a variety of system services. It was introduced with [Systemd](\(urls.systemd)) to help system administrators collect, access, and route log data."
}
