package metadata

services: snmp: {
	name:     "SNMP"
	thing:    "an \(name) agent"
	url:      urls.snmp
	versions: null

	description: "[SNMP](\(urls.snmp)) (Simple Network Management Protocol) is a standard protocol used for network management and monitoring. SNMP traps are unsolicited messages sent from network devices to a management station to report events such as failures, threshold violations, or status changes."
}
