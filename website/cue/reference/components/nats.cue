package metadata

components: _nats: {
	features: {
		collect: from: {
			service: services.nats
			interface: {
				socket: {
					api: {
						title: "NATS protocol"
						url:   urls.nats
					}
					direction: "incoming"
					port:      4222
					protocols: ["tcp"]
					ssl: "optional"
				}
			}
		}

		send: to: {
			service: services.nats
			interface: {
				socket: {
					api: {
						title: "NATS protocol"
						url:   urls.nats
					}
					direction: "outgoing"
					protocols: ["tcp"]
					ssl: "optional"
				}
			}
		}
	}

	support: {
		requirements: []
		notices: []
		warnings: []
	}

	configuration: {
		url: {
			description: "The NATS URL to connect to. The url _must_ take the form of `nats://server:port`."
			required:    true
			type: string: {
				examples: ["nats://demo.nats.io", "nats://127.0.0.1:4222"]
			}
		}
		subject: {
			description: "The NATS subject to publish messages to."
			required:    true
			type: string: {
				examples: ["{{ host }}", "foo", "time.us.east", "time.*.east", "time.>", ">"]
				syntax: "template"
			}
		}
		connection_name: {
			common:      false
			description: "A name assigned to the NATS connection."
			required:    false
			type: string: {
				default: "vector"
				examples: ["foo", "API Name Option Example"]
			}
		}
		auth: {
			common:      false
			description: "Configuration for how Vector should authenticate to NATS."
			required:    false
			type: object: options: {
				strategy: {
					common:      false
					description: "The strategy used to uniquely identify files. See https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro. For TLS Client Certiificate Auth, use the TLS configuration."
					required:    false
					type: string: {
						default: ""
						enum: {
							user_password:    "Username and password auth: https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro/username_password"
							token:            "Token auth: https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro/tokens"
							credentials_file: "Credentials file auth: https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro/jwt"
							nkey:             "Nkey auth: https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro/nkey_auth"
						}
					}
				}
				username: {
					common:        false
					description:   "username"
					relevant_when: "strategy = \"user_password\""
					required:      false
					type: string: {
						default: ""
						examples: ["nats-user"]
					}
				}
				password: {
					common:        false
					description:   "password"
					relevant_when: "strategy = \"user_password\""
					required:      false
					type: string: {
						default: ""
						examples: ["nats-password"]
					}
				}
				token: {
					common:        false
					description:   "token"
					relevant_when: "strategy = \"token\""
					required:      false
					type: string: {
						default: ""
						examples: ["secret-token"]
					}
				}
				credentials_file: {
					common:        false
					description:   "Path to credentials file"
					relevant_when: "strategy = \"credentials_file\""
					required:      false
					type: string: {
						default: ""
						examples: ["/etc/nats/nats.creds"]
					}
				}
				nkey: {
					common:        false
					description:   "User string representing nkey public key"
					relevant_when: "strategy = \"nkey\""
					required:      false
					type: string: {
						default: ""
						examples: ["UDXU4RCSJNZOIQHZNWXHXORDPRTGNJAHAHFRGZNEEJCPQTT2M7NLCNF4"]
					}
				}
				seed: {
					common:        false
					description:   "Seed string representing nkey private key"
					relevant_when: "strategy = \"nkey\""
					required:      false
					type: string: {
						default: ""
						examples: ["SUACSSL3UAHUDXKFSNVUZRF5UHPMWZ6BFDTJ7M6USDXIEDNPPQYYYCU3VY"]
					}
				}
			}
		}
	}

	how_it_works: {
		nats_rs: {
			title: "nats.rs"
			body:  """
				The `nats` source/sink uses [`nats.rs`](\(urls.nats_rs)) under the hood.
				"""
		}
	}
}
