package metadata

components: sources: snmp_trap: {
	_port: 162

	title: "SNMP Trap"

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		deployment_roles: ["aggregator"]
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		auto_generated:   true
		acknowledgements: false
		multiline: enabled: false
		receive: {
			from: {
				service: services.snmp

				interface: socket: {
					api: {
						title: "SNMP"
						url:   urls.snmp
					}
					direction: "incoming"
					port:      _port
					protocols: ["udp"]
					ssl: "disabled"
				}
			}
			receive_buffer_bytes: enabled: true
			keepalive: enabled:            false
			tls: enabled:                  false
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: generated.components.sources.snmp_trap.configuration

	output: logs: trap: {
		description: "An individual SNMP trap event"
		fields: {
			snmp_version: {
				description: "The SNMP version of the trap message."
				required:    true
				type: string: {
					examples: ["1", "2c"]
				}
			}
			source_address: {
				description: "The IP address and port of the SNMP agent that sent the trap."
				required:    true
				type: string: {
					examples: ["192.168.1.100:161"]
				}
			}
			community: {
				description: "The SNMP community string from the trap message."
				required:    true
				type: string: {
					examples: ["public", "private"]
				}
			}
			enterprise_oid: {
				description: "The enterprise OID identifying the device type (SNMPv1 only)."
				required:    false
				type: string: {
					examples: ["1.3.6.1.4.1.8072.2.3.0.1"]
				}
			}
			agent_address: {
				description: "The IP address of the SNMP agent (SNMPv1 only)."
				required:    false
				type: string: {
					examples: ["192.168.1.100"]
				}
			}
			generic_trap: {
				description: "The generic trap type (SNMPv1 only). Values: 0=coldStart, 1=warmStart, 2=linkDown, 3=linkUp, 4=authenticationFailure, 5=egpNeighborLoss, 6=enterpriseSpecific."
				required:    false
				type: uint: {
					examples: [0, 1, 2, 3, 4, 5, 6]
					unit: null
				}
			}
			specific_trap: {
				description: "The specific trap code (SNMPv1 only)."
				required:    false
				type: uint: {
					examples: [1, 2, 100]
					unit: null
				}
			}
			trap_oid: {
				description: "The trap OID identifying the trap type (SNMPv2c only)."
				required:    false
				type: string: {
					examples: ["1.3.6.1.6.3.1.1.5.1"]
				}
			}
			request_id: {
				description: "The request ID from the trap message (SNMPv2c only)."
				required:    false
				type: uint: {
					examples: [12345]
					unit: null
				}
			}
			uptime: {
				description: "The system uptime when the trap was generated."
				required:    false
				type: string: {
					examples: ["123456"]
				}
			}
			varbinds: {
				description: "An array of variable bindings containing OID-value pairs from the trap."
				required:    true
				type: array: items: type: object: options: {
					oid: {
						description: "The OID of the variable."
						required:    true
						type: string: {}
					}
					value: {
						description: "The value of the variable."
						required:    true
						type: string: {}
					}
				}
			}
			message: {
				description: "A human-readable summary of the trap."
				required:    true
				type: string: {
					examples: ["SNMPv1 trap from 192.168.1.100:161 (1.3.6.1.4.1.8072.2.3.0.1): coldStart"]
				}
			}
			timestamp: {
				description: "The time the trap was received by Vector."
				required:    true
				type: timestamp: {}
			}
		}
	}

	examples: [
		{
			title: "SNMPv2c Trap"
			configuration: {
				address: "0.0.0.0:162"
			}
			input: "[Binary SNMP trap data]"
			output: log: {
				snmp_version:   "2c"
				source_address: "192.168.1.100:161"
				community:      "public"
				request_id:     12345
				trap_oid:       "1.3.6.1.4.1.8072.2.3.0.1"
				uptime:         "123456"
				varbinds: [
					{oid: "1.3.6.1.2.1.1.3.0", value: "123456"},
					{oid: "1.3.6.1.6.3.1.1.4.1.0", value: "1.3.6.1.4.1.8072.2.3.0.1"},
				]
				message:   "SNMPv2c trap from 192.168.1.100:161: 1.3.6.1.4.1.8072.2.3.0.1"
				timestamp: "2024-01-15T10:30:00Z"
			}
		},
	]

	how_it_works: {
		snmp_versions: {
			title: "Supported SNMP Versions"
			body: """
				This source supports SNMPv1 and SNMPv2c trap messages. SNMPv3 traps are not
				currently supported due to the complexity of the security model.

				SNMPv1 traps contain enterprise OID, agent address, generic trap type, and
				specific trap code fields. SNMPv2c traps use a different format with trap OID
				and request ID fields.
				"""
		}

		community_strings: {
			title: "Community Strings"
			body: """
				SNMP community strings are included in the parsed output. Note that SNMPv1 and
				SNMPv2c community strings are sent in plaintext and provide minimal security.
				Consider using network-level security measures when receiving SNMP traps.
				"""
		}

		variable_bindings: {
			title: "Variable Bindings"
			body: """
				Variable bindings (varbinds) contain the actual data in the trap message. Each
				varbind consists of an OID and a value. The values are converted to strings
				for consistency, regardless of the original SNMP data type (Integer, OctetString,
				Counter32, etc.).
				"""
		}

		port_privileges: {
			title: "Port Privileges"
			body: """
				The standard SNMP trap port (162) is a privileged port on Unix systems. You may
				need to run Vector with elevated privileges or use a non-privileged port
				(e.g., 1162) and configure your network to forward traps accordingly.
				"""
		}
	}

	telemetry: metrics: {
		component_received_bytes: components.sources.internal_metrics.output.metrics.component_received_bytes
	}
}
