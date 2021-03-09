package metadata

components: sources: dnstap: {
	title: "Dnstap"

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		deployment_roles: ["daemon"]
		development:   "stable"
		egress_method: "stream"
	}

	features: {
		multiline: enabled: false
		receive: {
			from: {
				service: services.bind_dnstap
				interface: socket: {
					api: {
						title: "dnstap"
						url:   urls.dnstap
					}
					direction: "incoming"
					port:      0
					protocols: ["unix"]
					socket: "/run/bind/dnstap.sock"
					ssl: "disabled"
				}
			}
			tls: enabled: false
		}
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      false
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		max_frame_length: {
			common:      false
			description: "Max dnstap frame length that the dns source can handle."
			required: false
			type: uint: {
				default: 102400
				unit:    "bytes"
			}
		}
		socket_path: {
			description: """
				Absolute path of server socket file to which BIND is configured
				to send dnstap data. The socket file will be created by dnstap 
				source component automatically upon startup.
				"""
			required: true
			type: string: {
				examples: ["/run/bind/dnstap.sock"]
			}
		}
		socket_file_mode: {
			common: true
			description: """
				Unix file mode bits to be applied to server socket file 
				as its designated file permissions.
				"""
			required: false
			type: uint: {
				default: null
				unit: null
				examples: [0o777, 0o754, 508]
			}
		}
		multithreaded: {
			common: false
			description: """
				Whether or not to spawn a new asynchronous task for each dnstap 
				frame. This is to enable concurrent dnstap frame handling.
				"""
			required: false
			type: bool: default: false
			warnings: [
				"""
					Enabling concurrent dnstap frame handling may increase memory 
					consumption significantly. To limit memory usage, 
					set \"max_frame_handling_tasks\" accordingly.
					"""
			]
		}
		max_frame_handling_tasks: {
			common: false
			description: """
				Max number of outstanding dnstap frame handling tasks to be allowed
				at any given time.
				"""
			required: false
			type: uint: {
				default: 1000
			}
			warnings: [
				"""
					Once the limit is reached, the reading of incoming dnstap data 
					will be paused until the number of outstanding tasks decreases 
					below the limit.
					""",
				"""
					Upraising the limit may increase memory consumption. Be cautious!
					"""
			]
		}
		socket_receive_buffer_size: {
			common: false
			description: """
				Set receive buffer size of server Unix socket if specified. 
				No change to the default size if omitted.
				"""
			required: false
			type: uint: {
				default: null
				unit: "bytes"
			}
			warnings: [
				"""
					System-wide setting of max socket receive buffer size 
					(i.e. value of '/proc/sys/net/core/rmem_max' on Linux) 
					may need adjustment accordingly.
					"""
			]
		}
		socket_send_buffer_size: {
			common: false
			description: """
				Set send buffer size of server Unix socket if specified. 
				No change to the default size if omitted.
				"""
			required: false
			type: uint: {
				default: null
				unit: "bytes"
			}
			warnings: [
				"""
					System-wide setting of max socket send buffer size 
					(i.e. value of '/proc/sys/net/core/wmem_max' on Linux) 
					may need adjustment accordingly.
					"""
			]
		}
		raw_data_only: {
			common: false
			description: """
				Whether or not to write out raw dnstap frame data directly 
				(to be encoded in Base64) without any parsing and formatting.
				"""
			required: false
			type: bool: default: false
		}
	}

	output: logs: event: {
		description: "A single dnstap event."
		fields: {
			dataType: {
				common: true
				description: "Dnstap event data type. Currently only 'Message' type is defined."
				required:    false
				type: string: {
					enum: {
						Message: "Payload is a dnstap message."
					}
				}
			}
			dataTypeId: {
				description: "Numeric ID of dnstap event data type."
				required:    true
				type: uint: {
					unit: null
					examples: [1]
				}				
			}
			messageType: {
				relevant_when: "dataTypeId = 1"
				common: true
				description: "Dnstap message type."
				required: false
				type: string: {
					enum: {
						AuthQuery: """
							A DNS query message received from a resolver by an 
							authoritative name server, from the perspective of 
							the authoritative name server.
							"""
						AuthResponse: """
							A DNS response message sent from an authoritative 
							name server to a resolver, from the perspective of 
							the authoritative name server.
							"""
						ResolverQuery: """
							A DNS query message sent from a resolver to an 
							authoritative name server, from the perspective 
							of the resolver. Resolvers typically clear the 
							RD (recursion desired) bit when sending queries.
							"""
						ResolverResponse: """
							A DNS response message received from an authoritative 
							name server by a resolver, from the perspective of the 
							resolver.
							"""
						ClientQuery: """
							A DNS query message sent from a client to a DNS server 
							which is expected to perform further recursion, from 
							the perspective of the DNS server. The client may be 
							a stub resolver or forwarder or some other type of 
							software which typically sets the RD (recursion desired) 
							bit when querying the DNS server. The DNS server may be 
							a simple forwarding proxy or it may be a full recursive 
							resolver.
							"""
						ClientResponse: """
							A DNS response message sent from a DNS server to a client, 
							from the perspective of the DNS server. The DNS server 
							typically sets the RA(recursion available) bit when 
							responding.
							"""
						ForwarderQuery: """
							A DNS query message sent from a downstream DNS server to 
							an upstream DNS server which is expected to perform 
							further recursion, from the perspective of the downstream 
							DNS server.
							"""
						ForwarderResponse: """
							A DNS response message sent from an upstream DNS server 
							performing recursion to a downstream DNS server, from 
							the perspective of the downstream DNS server.
							"""
						StubQuery: """
							A DNS query message sent from a stub resolver to a DNS 
							server, from the perspective of the stub resolver.
							"""
						StubResponse: """
							A DNS response message sent from a DNS server to a stub 
							resolver, from the perspective of the stub resolver.
							"""
						ToolQuery: """
							A DNS query message sent from a DNS software tool to a 
							DNS server, from the perspective of the tool.
							"""
						ToolResponse: """
							A DNS response message received by a DNS software tool 
							from a DNS server, from the perspective of the tool.
							"""
						UpdateQuery: """
							A DNS update query message received from a resolver by 
							an authoritative name server, from the perspective of 
							the authoritative name server.
							"""
						UpdateResponse: """
							A DNS update response message sent from an authoritative 
							name server to a resolver, from the perspective of the 
							authoritative name server.
							"""
					}
				}
			}
			messageTypeId: {
				relevant_when: "dataTypeId = 1"
				description: "Numeric ID of dnstap message type."
				required:    true
				type: uint: {
					unit: null
					examples: [6]
				}				
			}
			time: {
				relevant_when: "dataTypeId = 1"
				description: """
					The time at which the DNS message was sent or received. 
					This is the number of time units (determined by 'timePrecision') 
					since the UNIX epoch. For a DNS query/update request event, 
					it's same as request time. For a DNS query/update response event, 
					it's same as response time.
					"""
				required: true
				type: uint: {
					unit: null
					examples: [1614781642516276825]
				}
			}
			timePrecision: {
				relevant_when: "dataTypeId = 1"
				description: "The time precision used by field 'time'."
				required: true
				type: string: {
					enum: {
						s: "second"
						ms: "millisecond"
						us: "microsecond"
						ns: "nanosecond"
					}
				}
			}
			serverId: {
				common: true
				description: "DNS server identity."
				required: false
				type: string: {
					examples: ["ns1.example.com"]
				}
			}
			serverVersion: {
				common: true
				description: "DNS server version."
				required: false
				type: string: {
					examples: ["BIND 9.16.8"]
				}
			}
			extraInfo: {
				common: false
				description: "Extra data for this event."
				required: false
				type: string: {
					examples: ["an arbitrary byte-string annotation"]
				}
			}
			socketFamily: {
				relevant_when: "dataTypeId = 1"
				description: """
					The network protocol family of a socket. This specifies how 
					to interpret 'sourceAddress'/'responseAddress' fields.
					"""
				required: true
				type: string: {
					enum: {
						INET: "IPv4 (RFC 791)."
						INET6: "IPv6 (RFC 2460)."
					}
				}
			}
			socketProtocol: {
				relevant_when: "dataTypeId = 1"
				description: """
					The transport protocol of a socket. This specifies how to 
					interpret 'sourcePort'/'responsePort' fields.
					"""
				required: true
				type: string: {
					enum: {
						UDP: "User Datagram Protocol (RFC 768)."
						TCP: "Transmission Control Protocol (RFC 793)."
					}
				}
			}
			sourceAddress: {
				relevant_when: "dataTypeId = 1"
				description: "The network address of the message initiator."
				required: true
				type: string: {
					examples: ["192.0.2.8", "fc00::100"]
				}
			}
			sourcePort: {
				relevant_when: "dataTypeId = 1"
				common: true
				description: "The transport port of the message initiator."
				required: false
				type: uint: {
					default: 0
					examples: [52398]
				}
			}
			responseAddress: {
				relevant_when: "dataTypeId = 1"
				description: "The network address of the message responder."
				required: true
				type: string: {
					examples: ["192.0.2.18", "fc00::200"]
				}
			}
			responsePort: {
				relevant_when: "dataTypeId = 1"
				common: true
				description: "The transport port of the message responder."
				required: false
				type: uint: {
					default: 0
					examples: [60364]
				}
			}
			error: {
				common: false
				description: "Error message upon failure while parsing dnstap data."
				required: false
				type: string: {
					examples: ["Encountered error : Unexpected number of records in update section: 0"]
				}
			}
			rawData: {
				common: false
				description: """
					Raw dnstap binary data encoded in Base64. Presents only upon 
					failures or option 'raw_data_only' is enabled.
					"""
				required: false
				type: string: {
					examples: ["ChBqYW1lcy11YnVudHUtZGV2EgtCSU5EIDkuMTYuNXKdAQgCEAEYASIEfwAAASoEfwAAATDRyAM4AFoNB2V4YW1wbGUDY29tAGCTvf76BW3evGImcmlihYQAAAEAAAABAAACaDIHZXhhbXBsZQNjb20AAAYAAcAPAAYAAQAADhAAPQtiZGRzLWRuc3RhcAAKcG9zdG1hc3RlcgJubwVlbWFpbAZwbGVhc2UAJADGPgAADhAAAAJYACeNAAAADhB4AQ=="]
				}
			}
			requestData: {
				relevant_when: "dataTypeId = 1"
				common: true
				description: "Request message data for DNS query/update."
				required: false
				type: object: {
					options: {
						time: {
							description: """
								The time at which the DNS query/update request message 
								was sent or received. This is the number of time units 
								(determined by 'timePrecision') since the UNIX epoch.
								"""
							required: true
							type: uint: {
								unit: null
								examples: [1614781642516276825]
							}
						}
						timePrecision: {
							description: "The time precision used by field 'time'."
							required: true
							type: string: {
								enum: {
									s: "second"
									ms: "millisecond"
									us: "microsecond"
									ns: "nanosecond"
								}
							}
						}
						fullRcode: {
							common: true
							description: """
								The numeric rcode that is the sum of the 4bits header's 
								rcode + the 8bits opt's extendedRcode when present. 
								Should be 0 for request.
								"""
							required: false
							type: uint: {
								unit: null
								examples: [0]
							}
						}
						rcodeName: {
							common: true
							description: """
								Textual response code corresponding to the 'fullRcode'. 
								Should be 'No Error' for request.
								"""
							required: false
							type: string: {
								enum: {
									NoError: "No Error"
									FormErr: "Format Error"                      
									ServFail: "Server Failure"
									NXDomain: "Non-Existent Domain"
									NotImp: "Not Implemented"
									Refused: "Query Refused"
									YXDomain: "Name Exists when it should not"
									YXRRSet: "RR Set Exists when it should not"
									NXRRSet: "RR Set that should exist does not"
									NotAuth: "Server Not Authoritative for zone"
									NotZone: "Name not contained in zone"
									BADSIG: "TSIG Signature Failure"
									BADKEY: "Key not recognized"
									BADTIME: "Signature out of time window"
									BADMODE: "Bad TKEY Mode"
									BADNAME: "Duplicate key name"
									BADALG: "Algorithm not supported"
									BADTRUNC: "Bad Truncation"
									BADCOOKIE: "Bad/missing server cookie"
								}
							}
						}
						rawData: {
							common: false
							description: """
								Raw binary request message data encoded in Base64. 
								Presents only upon failures.
								"""
							required: false
							type: string: {
								examples: ["YoWEAAABAAAAAQAAAmgyB2V4YW1wbGUDY29tAAAGAAHADwAGAAEAAA4QAD0LYmRkcy1kbnN0YXAACnBvc3RtYXN0ZXICbm8FZW1haWwGcGxlYXNlACQAxj4AAA4QAAACWAAnjQAAAA4Q"]
							}
						}
						header: {
							common: true
							description: """
								Header section of DNS query/update request message. 
								See DNS related RFCs for detailed information about 
								its content.
								"""
							required: false
							type: object: {
								examples: [
									{
										"aa": false,
										"ad": true,
										"anCount": 0,
										"arCount": 1,
										"cd": false,
										"id": 3341,
										"nsCount": 0,
										"opcode": 0,
										"qdCount": 1,
										"qr": 0,
										"ra": false,
										"rcode": 0,
										"rd": true,
										"tc": false
									}
								]
								options: {}
							}
						}
						question: {
							common:true
							description: """
								Question section of DNS query request message. See DNS 
								related RFCs for detailed information about its content.
								"""
							required: false
							type: object: {
								examples: [
									{
										"class": "IN",
										"domainName": "host.example.com.",
										"questionType": "A",
										"questionTypeId": 1
									}
								]
								options: {}
							}
						}
						additional: {
							common:true
							description: """
								Additional section of DNS query request message. See DNS 
								related RFCs for detailed information about its content.
								"""
							required: false
							type: object: {
								examples: [
									{
										"class": "IN",
										"domainName": "ns.example.com.",
										"rData": "192.0.2.1",
										"recordType": "A",
										"recordTypeId": 1,
										"ttl": 3600
									}
								]
								options: {}
							}
						}
						opt: {
							common:true
							description: """
								A pseudo section containing EDNS options of DNS query request 
								message. See DNS related RFCs for detailed information about 
								its content.
								"""
							required: false
							type: object: {
								examples: [
									{
										"do": false,
										"ednsVersion": 0,
										"extendedRcode": 0,
										"options": [
											{
												"optCode": 10,
												"optName": "Cookie",
												"optValue": "hbbDFmHUM9w="
											}
										],
										"udpPayloadSize": 4096
									}
								]
								options: {}
							}
						}
						zone: {
							common:true
							description: """
								Zone section of DNS update request message. See DNS related 
								RFCs for detailed information about its content.
								"""
							required: false
							type: object: {
								examples: [
									{
										"zClass": "IN",
										"zName": "example.com.",
										"zType": "SOA",
										"zTypeId": 6
									}
								]
								options: {}
							}
						}
						prerequisite: {
							common:true
							description: """
								Prerequisite section of DNS update request message. See DNS 
								related RFCs for detailed information about its content.
								"""
							required: false
							type: object: {
								examples: [
									{
										"class": "IN",
										"domainName": "host.example.com.",
										"rData": "192.0.2.100",
										"recordType": "A",
										"recordTypeId": 1
									}
								]
								options: {}
							}
						}
						update: {
							common:true
							description: """
								Update section of DNS update request message. See DNS related 
								RFCs for detailed information about its content.
								"""
							required: false
							type: object: {
								examples: [
									{
										"class": "IN",
										"domainName": "h1.example.com.",
										"rData": "192.0.2.110",
										"recordType": "A",
										"recordTypeId": 1,
										"ttl": 3600
									}
								]
								options: {}
							}
						}
					}
				}
			}
			responseData: {
				relevant_when: "dataTypeId = 1"
				common: true
				description: "Response message data for DNS query/update."
				required: false
				type: object: {
					options: {
						time: {
							description: """
								The time at which the DNS query/update response message was 
								sent or received. This is the number of time units (determined 
								by 'timePrecision') since the UNIX epoch.
								"""
							required: true
							type: uint: {
								unit: null
								examples: [1614781642516276825]
							}
						}
						timePrecision: {
							description: "The time precision used by field 'time'."
							required: true
							type: string: {
								enum: {
									s: "second"
									ms: "millisecond"
									us: "microsecond"
									ns: "nanosecond"
								}
							}
						}
						fullRcode: {
							common: true
							description: """
								The numeric rcode that is the sum of the 4bits header's 
								rcode + the 8bits opt's extendedRcode when present.
								"""
							required: false
							type: uint: {
								unit: null
								examples: [0, 5]
							}
						}
						rcodeName: {
							common: true
							description: "Textual response code corresponding to the 'fullRcode'"
							required: false
							type: string: {
								enum: {
									NoError: "No Error"
									FormErr: "Format Error"                      
									ServFail: "Server Failure"
									NXDomain: "Non-Existent Domain"
									NotImp: "Not Implemented"
									Refused: "Query Refused"
									YXDomain: "Name Exists when it should not"
									YXRRSet: "RR Set Exists when it should not"
									NXRRSet: "RR Set that should exist does not"
									NotAuth: "Server Not Authoritative for zone"
									NotZone: "Name not contained in zone"
									BADSIG: "TSIG Signature Failure"
									BADKEY: "Key not recognized"
									BADTIME: "Signature out of time window"
									BADMODE: "Bad TKEY Mode"
									BADNAME: "Duplicate key name"
									BADALG: "Algorithm not supported"
									BADTRUNC: "Bad Truncation"
									BADCOOKIE: "Bad/missing server cookie"
								}
							}
						}
						rawData: {
							common: false
							description: """
								Raw binary response message data encoded in Base64. 
								Presents only upon failures.
								"""
							required: false
							type: string: {
								examples: ["YoWEAAABAAAAAQAAAmgyB2V4YW1wbGUDY29tAAAGAAHADwAGAAEAAA4QAD0LYmRkcy1kbnN0YXAACnBvc3RtYXN0ZXICbm8FZW1haWwGcGxlYXNlACQAxj4AAA4QAAACWAAnjQAAAA4Q"]
							}
						}
						header: {
							common: true
							description: """
								Header section of DNS query/update response message. 
								See DNS related RFCs for detailed information about 
								its content.
								"""
							required: false
							type: object: {
								examples: [
									{
										"aa": true,
										"ad": false,
										"anCount": 1,
										"arCount": 0,
										"cd": false,
										"id": 49653,
										"nsCount": 1,
										"opcode": 0,
										"qdCount": 1,
										"qr": 1,
										"ra": true,
										"rcode": 0,
										"rd": true,
										"tc": false
									}
								]
								options: {}
							}
						}
						question: {
							common:true
							description: """
								Question section of DNS query response message. See DNS 
								related RFCs for detailed information about its content.
								"""
							required: false
							type: object: {
								examples: [
									{
										"class": "IN",
										"domainName": "host.example.com.",
										"questionType": "A",
										"questionTypeId": 1
									}
								]
								options: {}
							}
						}
						answers: {
							common:true
							description: """
								Answers section of DNS query response message. See DNS 
								related RFCs for detailed information about its content.
								"""
							required: false
							type: object: {
								examples: [
									{
										"class": "IN",
										"domainName": "host.example.com.",
										"rData": "192.0.2.100",
										"recordType": "A",
										"recordTypeId": 1,
										"ttl": 3600
									}
								]
								options: {}
							}
						}
						authority: {
							common:true
							description: """
								Authority section of DNS query response message. See DNS 
								related RFCs for detailed information about its content.
								"""
							required: false
							type: object: {
								examples: [
									{
										"class": "IN",
										"domainName": "example.com.",
										"rData": "ns1.example.com.",
										"recordType": "NS",
										"recordTypeId": 2,
										"ttl": 86400
									}
								]
								options: {}
							}
						}
						additional: {
							common:true
							description: """
								Additional section of DNS query response message. See DNS 
								related RFCs for detailed information about its content.
								"""
							required: false
							type: object: {
								examples: [
									{
										"class": "IN",
										"domainName": "ns.example.com.",
										"rData": "192.0.2.1",
										"recordType": "A",
										"recordTypeId": 1,
										"ttl": 3600
									}
								]
								options: {}
							}
						}
						opt: {
							common:true
							description: """
								A pseudo section containing EDNS options of DNS query response 
								message. See DNS related RFCs for detailed information about 
								its content.
								"""
							required: false
							type: object: {
								examples: [
									{
										"do": false,
										"ednsVersion": 0,
										"extendedRcode": 0,
										"options": [
											{
												"optCode": 10,
												"optName": "Cookie",
												"optValue": "hbbDFmHUM9wBAAAAX1q1McL4KhalWTS3"
											}
										],
										"udpPayloadSize": 4096
									}
								]
								options: {}
							}
						}
						zone: {
							common:true
							description: """
								Zone section of DNS update response message. See DNS related 
								RFCs for detailed information about its content.
								"""
							required: false
							type: object: {
								examples: [
									{
										"zClass": "IN",
										"zName": "example.com.",
										"zType": "SOA",
										"zTypeId": 6
									}
								]
								options: {}
							}
						}
					}
				}
			}
		}
	}

	examples: [
		{
			title: "Dnstap events for a pair of regular DNS query and response."
			configuration: {
				max_frame_length: 102400
				socket_file_mode: 508
				socket_path: "/run/bind/dnstap.sock"
				multithreaded: true
				max_frame_handling_tasks: 10000
			}
			input: """
				Send a query to an authoritative BIND DNS server locally with following command:

				```bash
					nslookup host.example.com localhost
				```
				"""
			output: [
				{
					log: {
						"dataType": "Message",
						"dataTypeId": 1,
						"messageType": "ClientQuery",
						"messageTypeId": 5,
						"requestData": {
							"fullRcode": 0,
							"header": {
								"aa": false,
								"ad": false,
								"anCount": 0,
								"arCount": 0,
								"cd": false,
								"id": 49653,
								"nsCount": 0,
								"opcode": 0,
								"qdCount": 1,
								"qr": 0,
								"ra": false,
								"rcode": 0,
								"rd": true,
								"tc": false
							},
							"question": [
								{
									"class": "IN",
									"domainName": "host.example.com.",
									"questionType": "A",
									"questionTypeId": 1
								}
							],
							"rcodeName": "NoError",
							"time": 1614781642516276825,
							"timePrecision": "ns"
						},
						"responseAddress": "127.0.0.1",
						"responsePort": 0,
						"serverId": "ns1.example.com",
						"serverVersion": "BIND 9.16.8",
						"socketFamily": "INET",
						"socketProtocol": "UDP",
						"sourceAddress": "127.0.0.1",
						"sourcePort": 52398,
						"time": 1614781642516276825,
						"timePrecision": "ns"
					}
				},
				{
					log: {
						"dataType": "Message",
						"dataTypeId": 1,
						"messageType": "ClientResponse",
						"messageTypeId": 6,
						"responseAddress": "127.0.0.1",
						"responseData": {
							"answers": [
								{
									"class": "IN",
									"domainName": "host.example.com.",
									"rData": "192.0.2.100",
									"recordType": "A",
									"recordTypeId": 1,
									"ttl": 3600
								}
							],
							"authority": [
								{
									"class": "IN",
									"domainName": "example.com.",
									"rData": "ns1.example.com.",
									"recordType": "NS",
									"recordTypeId": 2,
									"ttl": 86400
								}
							],
							"fullRcode": 0,
							"header": {
								"aa": true,
								"ad": false,
								"anCount": 1,
								"arCount": 0,
								"cd": false,
								"id": 49653,
								"nsCount": 1,
								"opcode": 0,
								"qdCount": 1,
								"qr": 1,
								"ra": true,
								"rcode": 0,
								"rd": true,
								"tc": false
							},
							"question": [
								{
									"class": "IN",
									"domainName": "host.example.com.",
									"questionType": "A",
									"questionTypeId": 1
								}
							],
							"rcodeName": "NoError",
							"time": 1614781642516276825,
							"timePrecision": "ns"
						},
						"responsePort": 0,
						"serverId": "ns1.example.com",
						"serverVersion": "BIND 9.16.8",
						"socketFamily": "INET",
						"socketProtocol": "UDP",
						"sourceAddress": "127.0.0.1",
						"sourceId": "421bce7d-b4e6-b705-6057-7039628a9847",
						"sourcePort": 52398,
						"time": 1614781642516276825,
						"timePrecision": "ns"
					}
				}
			]
			notes: """
				* The BIND DNS server should host zone \"example.com\"
				* Zone \"example.com\" contains a host record \"host.example.com\"
				* The BIND DNS server should have dnstap enabled and configured appropriately
				* Unix socket path configured in BIND and Vector should match each other
				* BIND should have 'rw' permissions to the Unix socket
				"""
		},
		{
			title: "Dnstap events for a pair of DNS update request and response."
			configuration: {
				socket_file_mode: 508
				socket_path: "/run/bind/dnstap.sock"
				socket_receive_buffer_size: 10485760
				socket_send_buffer_size: 10485760
			}
			input: """
				Send a dynamic update to an authoritative BIND DNS server locally with following command:

				```bash
					nsupdate <<EOF
					server localhost
					update add h1.example.com 3600 a 192.0.2.110
					send
					EOF
				```
				"""
			output: [
				{
					log: {
						"dataType": "Message",
						"dataTypeId": 1,
						"messageType": "UpdateQuery",
						"messageTypeId": 13,
						"responseAddress": "127.0.0.1",
						"responsePort": 0,
						"serverId": "ns1.example.com",
						"serverVersion": "BIND 9.16.8",
						"socketFamily": "INET",
						"socketProtocol": "UDP",
						"sourceAddress": "127.0.0.1",
						"sourcePort": 53141,
						"time": 1599832089886768480,
						"timePrecision": "ns",
						"requestData": {
							"fullRcode": 0,
							"header": {
							"adCount": 0,
							"id": 47320,
							"opcode": 5,
							"prCount": 0,
							"qr": 0,
							"rcode": 0,
							"upCount": 1,
							"zoCount": 1
							},
							"rcodeName": "NoError",
							"time": 1599832089886768480,
							"timePrecision": "ns",
							"update": [
							{
								"class": "IN",
								"domainName": "h1.example.com.",
								"rData": "192.0.2.110",
								"recordType": "A",
								"recordTypeId": 1,
								"ttl": 3600
							}
							],
							"zone": {
								"zClass": "IN",
								"zName": "example.com.",
								"zType": "SOA",
								"zTypeId": 6
							}
						}
					}
				},
				{
					log: {
						"dataType": "Message",
						"dataTypeId": 1,
						"messageType": "UpdateResponse",
						"messageTypeId": 14,
						"responseAddress": "127.0.0.1",
						"responsePort": 0,
						"serverId": "ns1.example.com",
						"serverVersion": "BIND 9.16.8",
						"socketFamily": "INET",
						"socketProtocol": "UDP",
						"sourceAddress": "127.0.0.1",
						"sourcePort": 53141,
						"time": 1599832089890768466,
						"timePrecision": "ns",
						"responseData": {
							"fullRcode": 0,
							"header": {
								"adCount": 0,
								"id": 47320,
								"opcode": 5,
								"prCount": 0,
								"qr": 1,
								"rcode": 0,
								"upCount": 0,
								"zoCount": 1
							},
							"rcodeName": "NoError",
							"time": 1599832089890768466,
							"timePrecision": "ns",
							"zone": {
								"zClass": "IN",
								"zName": "example.com.",
								"zType": "SOA",
								"zTypeId": 6
							}
						}
					}
				}
			]
			notes: """
				* The BIND DNS server should host zone \"example.com\"
				* Zone \"example.com\" should allow dynamic update
				* The BIND DNS server should have dnstap enabled and configured appropriately
				* Unix socket path configured in BIND and Vector should match each other
				* BIND should have 'rw' permissions to the Unix socket
				"""
		}
	]

	how_it_works: {
		server_uds: {
			title: "Server UDS (Unix Domain Socket)"
			body: #"""
				The dnstap source component receives dnstap data through a Unix
				domain socket (a.k.a. UDS). Path of the UDS must be specified
				explicitly in dnstap source configuration.

				Upon startup, the dnstap source component will create a new server
				UDS at the specified path. If the path of UDS has been occupied already,
				it'll be deleted first automatically.

				Default permissions of the UDS is determined by current "umask" value.
				To customize it to allow local BIND server to send dnstap data to the UDS,
				user can specify the desired UDS permissions (a.k.a. file mode) explicitly
				in dnstap source configuration. For example, to set its permission as "0774",
				add option "socket_file_mode" in dnstap source configuration like below:

				```toml
						[sources.my_dnstap_source]
							type = "dnstap"
							# ...
							socket_file_mode: 0o774
				```
				"""#
			sub_sections: [
				{
					title: "Work With Remote BIND server"
					body: #"""
						The dnstap source component can create server UDS only on local
						machine, but it's also possible to work with remote BIND server
						too. To do it, you'd have to forward the server UDS from vector's
						hosting machine to the remote BIND server (e.g. through "ssh") 
						once vector starts.
						Make sure the Unix domain sockets on both local and remote machines
						having appropriate permissions set.
						"""#
				}
			]
		}

		multithreaded_mode: {
			title: "Multi-threaded Mode"
			body: #"""
				By default, the dnstap source component reads and processes dnstap 
				data sequentially as a pipeline: retrieving a dnstap data frame, 
				parsing it thoroughly and composing a new vector event out of it,
				and finally sending the event composed onto the next vector component
				for further processing, then starting to retrieve next dnstap data frame.
				The pipeline should work fine with casual DNS query flows, but may 
				become inefficient on a multi-cpu-core machine under extreme load, 
				and cause inbound UDS channel to be jammed over time, which might in turn 
				cause BIND to start dropping dnstap data.

				To make the process more efficient on multi-cpu-core machine, we need to 
				spread the load to all available CPU cores as much as possible, especially
				the part of parsing dnstap data frame and composing vector event out of it.
				And that's exactly what the "multi-threaded mode" is for.

				When "multi-threaded mode" is enabled, retrieving of dnstap data frames
				out of UDS will still work as a pipeline. However, for each dnstap data
				frame read, an asynchronous task will be spawned for the rest of processing
				on the frame.

				The max number of outstanding asynchronous tasks (i.e. spawned but unfinished
				yet) allowed is regulated by option "max_frame_handling_tasks". Once the
				limit has reached, the frame retrieving pipeline will pause for a few
				milliseconds, and try again, until some tasks are finished, and the number of
				outstanding tasks decreases below the limit.

				To enable "multi-threaded mode" and configure the max number of outstanding 
				tasks allowed, add options "multithreaded" and "max_frame_handling_tasks" 
				in dnstap source configuration like below:

				```toml
						[sources.my_dnstap_source]
							type = "dnstap"
							# ...
							multithreaded = true
							max_frame_handling_tasks = 10_000
				```
				"""#
		}

		manipulate_uds_buffer_size: {
			title: "Manipulate UDS Buffer Size"
			body: #"""
				The dnstap source component supports configuring UDS buffer size for 
				both receiving and sending, which may be helpful to handle spiked DNS
				traffic more smoothly for some high performance use scenarios.

				To configure receive/send buffer size for the server UDS, add options 
				"socket_receive_buffer_size" and "socket_send_buffer_size" in dnstap 
				configuration accordingly, like below:

				```toml
						[sources.my_dnstap_source]
							type = "dnstap"
							# ...
							socket_receive_buffer_size = 10_485_760
							socket_send_buffer_size = 10_485_760
				```

				And certainly, for the buffer size settings to actually take effects,
				you'd have to ensure that the system-wide settings of max socket 
				receive/send buffer sizes (i.e. values of '/proc/sys/net/core/rmem_max'
				and '/proc/sys/net/core/wmem_max' on Linux) are large enough.
				"""#
		}
	}

}
