package metadata

components: sources: logstash: {
	_port: 5044

	title: "Logstash"

	classes: {
		commonly_used: true
		delivery:      "best_effort"
		deployment_roles: ["sidecar", "aggregator"]
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		receive: {
			from: {
				service: services.logstash

				interface: socket: {
					api: {
						title: "Logstash"
						url:   urls.logstash
					}
					direction: "incoming"
					port:      _port
					protocols: ["tcp"]
					ssl: "optional"
				}
			}
			receive_buffer_bytes: {
				enabled: true
			}
			keepalive: enabled: true
			tls: sources.socket.features.receive.tls
		}
		multiline: enabled: false
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.logstash.configuration

	output: logs: line: {
		description: "A Logstash message"
		fields: {
			host: {
				description: "The IP address the Logstash message was sent from."
				required:    true
				type: string: {
					examples: ["127.0.0.1"]
				}
			}
			timestamp: fields._current_timestamp & {
				description: """
					The timestamp field will be set to the first one found of the following:

					1. The `timestamp` field on the event
					2. The `@timestamp` field on the event if it can be parsed as a timestamp
					3. The current timestamp

					The assigned field, `timestamp`, could be different depending if you have configured
					`log_schema.timestamp_key`.
					"""
			}
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["logstash"]
				}
			}
			client_metadata: fields._client_metadata
			"*": {
				description: "In addition to the defined fields, all fields from the Logstash message are inserted as root level fields."
				required:    true
				type: string: {
					examples: ["hello world"]
				}
			}
		}
	}

	examples: [
		{
			title: "Logstash message from generator input"
			configuration: {}
			input: """
				Logstash input config:

				```text
				input {
					generator {
						count => 1
					}
				}
				```

				Output if sent to stdout logstash output:

				```text
				{ "@version" => "1", "@timestamp" => 2021-06-14T20:57:14.230Z, "host" => "c082bb583445", "sequence" => 0, "message" => "Hello world!" }
				```
				"""
			output: log: {
				host:        _values.remote_host
				line:        "2021-06-14T20:57:14.230Z c082bb583445 hello world"
				source_type: "logstash"
			}
		},
		{
			title: "Message from Elastic Beat Heartbeat agent"
			configuration: {}
			input: """
				Heartbeat input config:

				```yaml
				heartbeat.config.monitors:
					path: ${path.config}/monitors.d/*.yml
					reload.enabled: false
					reload.period: 5s

				heartbeat.monitors:
				- type: http
					schedule: '@every 5s'
					urls:
					- http://google.com
				```

				Output if sent to stdout output:

				```json
				{"@timestamp":"2021-06-14T21:25:37.058Z","@metadata":{"beat":"heartbeat","type":"_doc","version":"7.12.1"},"url":{"full":"http://google.com","scheme":"http","domain":"google.com","port":80},"tcp":{"rtt":{"connect":{"us":18504}}},"event":{"dataset":"uptime"},"ecs":{"version":"1.8.0"},"resolve":{"rtt":{"us":7200},"ip":"172.217.4.174"},"summary":{"up":1,"down":0},"http":{"response":{"mime_type":"text/html; charset=utf-8","headers":{"Content-Length":"219","Date":"Mon, 14 Jun 2021 21:25:37 GMT","Server":"gws","X-Xss-Protection":"0","Location":"http://www.google.com/","Expires":"Wed, 14 Jul 2021 21:25:37 GMT","Content-Type":"text/html; charset=UTF-8","Cache-Control":"public, max-age=2592000","X-Frame-Options":"SAMEORIGIN"},"status_code":301,"body":{"hash":"2178eedd5723a6ac22e94ec59bdcd99229c87f3623753f5e199678242f0e90de","bytes":219}},"rtt":{"response_header":{"us":51481},"validate":{"us":52664},"content":{"us":1182},"total":{"us":71585},"write_request":{"us":134}}},"monitor":{"type":"http","status":"up","duration":{"us":79517},"check_group":"0c8c908a-cd57-11eb-85a4-025000000001","ip":"172.217.4.174","timespan":{"gte":"2021-06-14T21:25:37.137Z","lt":"2021-06-14T21:25:42.137Z"},"id":"auto-http-0X993E1F882355CFD2","name":""},"agent":{"hostname":"docker-desktop","ephemeral_id":"9e15e5bc-86d6-4d47-9067-4262b00c5cce","id":"404c8975-a41b-45bd-8d93-3f6c4449e973","name":"docker-desktop","type":"heartbeat","version":"7.12.1"}}
				```
				"""
			output: log: {
				{
					"host":        _values.remote_host
					"timestamp":   "2021-06-14T21:25:37.058Z"
					"@timestamp":  "2021-06-14T21:25:37.058Z"
					"source_type": "logstash"
					"@metadata": {
						"beat":    "heartbeat"
						"type":    "_doc"
						"version": "7.12.1"
					}
					"url": {
						"full":   "http://google.com"
						"scheme": "http"
						"domain": "google.com"
						"port":   80
					}
					"tcp": {
						"rtt": {
							"connect": {
								"us": 18504
							}
						}
					}
					"event": {
						"dataset": "uptime"
					}
					"ecs": {
						"version": "1.8.0"
					}
					"resolve": {
						"rtt": {
							"us": 7200
						}
						"ip": "172.217.4.174"
					}
					"summary": {
						"up":   1
						"down": 0
					}
					"http": {
						"response": {
							"mime_type": "text/html; charset=utf-8"
							"headers": {
								"Content-Length":   "219"
								"Date":             "Mon, 14 Jun 2021 21:25:37 GMT"
								"Server":           "gws"
								"X-Xss-Protection": "0"
								"Location":         "http://www.google.com/"
								"Expires":          "Wed, 14 Jul 2021 21:25:37 GMT"
								"Content-Type":     "text/html; charset=UTF-8"
								"Cache-Control":    "public, max-age=2592000"
								"X-Frame-Options":  "SAMEORIGIN"
							}
							"status_code": 301
							"body": {
								"hash":  "2178eedd5723a6ac22e94ec59bdcd99229c87f3623753f5e199678242f0e90de"
								"bytes": 219
							}
						}
						"rtt": {
							"response_header": {
								"us": 51481
							}
							"validate": {
								"us": 52664
							}
							"content": {
								"us": 1182
							}
							"total": {
								"us": 71585
							}
							"write_request": {
								"us": 134
							}
						}
					}
					"monitor": {
						"type":   "http"
						"status": "up"
						"duration": {
							"us": 79517
						}
						"check_group": "0c8c908a-cd57-11eb-85a4-025000000001"
						"ip":          "172.217.4.174"
						"timespan": {
							"gte": "2021-06-14T21:25:37.137Z"
							"lt":  "2021-06-14T21:25:42.137Z"
						}
						"id":   "auto-http-0X993E1F882355CFD2"
						"name": ""
					}
					"agent": {
						"hostname":     "docker-desktop"
						"ephemeral_id": "9e15e5bc-86d6-4d47-9067-4262b00c5cce"
						"id":           "404c8975-a41b-45bd-8d93-3f6c4449e973"
						"name":         "docker-desktop"
						"type":         "heartbeat"
						"version":      "7.12.1"
					}
				}
			}
		},
	]

	how_it_works: {
		aggregator: {
			title: "Sending data from logstash agents to Vector aggregators"
			body: """
				If you are already running an Elastic agent (Logstash or Elastic Beats) in your infrastructure, this
				source can make it easy to start getting that data into Vector.
				"""
		}

		logstash_configuration: {
			title: "Logstash configuration"
			body: """
				To configure Logstash to forward to a Vector instance, you can use the following output configuration:

				```text
				output {
						lumberjack {
								# update these to point to your vector instance
								hosts => ["127.0.0.1"]
								port => 5044
								ssl_certificate => "/path/to/certificate.crt"
						}
				}
				```

				Note that Logstash requires SSL to be configured.
				"""
		}

		beats_configuration: {
			title: "Elastic Beats configuration"
			body: """
				To configure one of the Elastic Beats agents to forward to a Vector instance, you can use the following
				output configuration:

				```yaml
					output.logstash:
					  # update these to point to your vector instance
					  hosts: ["127.0.0.1:5044"]
				```
				"""
		}

		acking: {
			title: "Acknowledgement support"
			body: """
				Currently, this source will acknowledge events to the sender once the event has been sent to the next
				component in the topology. In the future, this source will utilize Vector's support for end-to-end
				acknowledgements.
				"""
		}
	}

	telemetry: metrics: {
		open_connections: components.sources.internal_metrics.output.metrics.open_connections
	}
}
