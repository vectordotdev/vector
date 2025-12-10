package metadata

generated: components: sources: snmp_trap: configuration: {
	address: {
		description: """
			The address to listen for SNMP traps on.

			SNMP traps are typically sent to UDP port 162.
			"""
		required: true
		type: string: examples: ["0.0.0.0:9000", "systemd", "systemd#3", "0.0.0.0:162", "127.0.0.1:1162"]
	}
	host_key: {
		description: """
			Overrides the name of the log field used to add the peer host to each event.

			The value is the peer host's address, including the port. For example, `192.168.1.1:162`.

			By default, the [global `log_schema.host_key` option][global_host_key] is used.

			[global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
			"""
		required: false
		type: string: {}
	}
	receive_buffer_bytes: {
		description: """
			The size of the receive buffer used for the listening socket.

			This should not typically need to be changed.
			"""
		required: false
		type: uint: unit: "bytes"
	}
}
