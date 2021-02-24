package metadata

data_model: schema: {
	log: {
		common: true
		description: """
			A Vector log event is a structured representation of a
			point-in-time event. It contains an arbitrary set of
			fields that describe the event.

			A key tenet of Vector is to remain schema neutral. This
			ensures that Vector can work with any schema, supporting
			legacy and future schemas as your needs evolve. Vector
			does not require any specific fields, and each component
			will document the fields it provides.
			"""
		required: false
		warnings: []
		type: object: {
			examples: [
				{
					"host":      "my.host.com"
					"message":   "Hello world"
					"timestamp": "2020-11-01T21:15:47+00:00"
					"custom":    "field"
				},
			]
			options: {
				"*": {
					common:      true
					description: "An arbitrary set of key/value pairs that can be infinitely nested."
					required:    false
					type: "*": {}
				}
			}
		}
	}

	metric: {
		common: true
		description: """
			A Vector metric event represents a numerical operation
			performed on a time series. Unlike other tools, metrics
			in Vector are first class citizens, they are not represented
			as structured logs. This makes them interoperable with
			various metrics services without the need for any
			transformation.

			Vector's metric data model favors accuracy and correctness over
			ideological purity. Therefore, Vector's metric types are a
			conglomeration of various metric types found in the wild, such as
			Prometheus and Statsd. This	ensures metric data is _correctly_
			interoperable between systems.
			"""
		required: false
		warnings: []
		type: object: {
			examples: []
			options: {
				counter: {
					common: true
					description: """
						A single value that can only be incremented
						or reset to zero value, it cannot be
						decremented.
						"""
					required: false
					warnings: []
					type: object: {
						examples: []
						options: {
							value: {
								description: "The value to increment the counter by. Can only be positive."
								required:    true
								warnings: []
								type: float: {
									examples: [1.0, 10.0, 500.0]
								}
							}
						}
					}
				}

				distribution: {
					common: true
					description: """
						A distribution represents a distribution of
						sampled values. It is used with services
						that support global histograms and summaries.
						"""
					required: false
					warnings: []
					type: object: {
						examples: []
						options: {
							samples: {
								description: "The set of sampled values."
								required:    true
								warnings: []
								type: array: items: type: object: {
									examples: []
									options: {
										rate: {
											description: "The rate at which this value was sampled."
											required:    true
											warnings: []
											type: uint: {
												examples: [12, 43, 25]
												unit: null
											}
										}
										value: {
											description: "The value being sampled."
											required:    true
											warnings: []
											// FIXME: making this float, as it should be, makes cue blow up
											type: uint: {
												// FIXME: Adding even empty examples makes cue blow up
												// examples: [12.0, 43.3, 25.2]
												unit: null
											}
										}
									}
								}
							}
							statistic: {
								description: "The statistic to be calculated from the values."
								required:    true
								warnings: []
								type: string: {
									enum: {
										histogram: "Counts values in buckets."
										summary:   "Calculates quantiles of values."
									}
									syntax: "literal"
								}
							}
						}
					}
				}

				gauge: {
					common: true
					description: """
						A gauge represents a point-in-time value
						that can increase and decrease. Vector's
						internal gauge type represents changes to
						that value. Gauges should be used to track
						fluctuations in values, like current memory
						or CPU usage.
						"""
					required: false
					warnings: []
					type: object: {
						examples: []
						options: {
							value: {
								description: "A specific point-in-time value for the gauge."
								required:    true
								warnings: []
								type: float: {
									examples: [1.0, 10.0, 500.0]
								}
							}
						}
					}
				}

				histogram: {
					common: true
					description: """
						Also called a "timer". A histogram samples
						observations (usually things like request
						durations or response sizes) and counts them
						in configurable buckets. It also provides a
						sum of all observed values.
						"""
					required: false
					warnings: []
					type: object: {
						examples: []
						options: {
							buckets: {
								description: "The set of buckets containing the histogram values."
								required:    true
								warnings: []
								type: array: items: type: object: {
									examples: []
									options: {
										count: {
											description: "The number of values contained within this bucket."
											required:    true
											warnings: []
											type: uint: {
												examples: [1, 10, 25, 100]
												unit: null
											}
										}
										upper_limit: {
											description: "The upper limit of the samples within the bucket."
											required:    true
											warnings: []
											// FIXME: making this float, as it should be, makes cue blow up
											type: uint: {
												// FIXME: Adding even empty examples makes cue blow up
												// examples: [12.0, 43.3, 25.2]
												unit: null
											}
										}
									}
								}
							}
							count: {
								description: "The total number of values contained within the histogram."
								required:    true
								warnings: []
								type: uint: {
									examples: [1, 10, 25, 100]
									unit: null
								}
							}
							sum: {
								description: "The sum of all values contained within the histogram."
								required:    true
								warnings: []
								type: float: {
									examples: [1.0, 10.0, 25.0, 100.0]
								}
							}
						}
					}
				}

				"kind": {
					description: "The metric value kind."
					required:    true
					warnings: []
					type: string: {
						enum: {
							absolute:    "The metric value is absolute and replaces values as it is received downstream."
							incremental: "The metric value increments a cumulated value as it is received downstream."
						}
						syntax: "literal"
					}
				}

				"name": {
					description: "The metric name."
					required:    true
					warnings: []
					type: string: {
						examples: ["memory_available_bytes"]
						syntax: "literal"
					}
				}

				"namespace": {
					description: "The metric namespace. Depending on the service, this will prepend the name or use native namespacing facilities."
					required:    true
					warnings: []
					type: string: {
						examples: ["host", "apache", "nginx"]
						syntax: "literal"
					}
				}

				set: {
					common: true
					description: """
						A set represents an array of unique values.
						"""
					required: false
					warnings: []
					type: object: {
						examples: []
						options: {
							values: {
								description: "The list of unique values."
								required:    true
								warnings: []
								type: array: items: type: string: {
									examples: ["value1", "value2"]
									syntax: "literal"
								}
							}
						}
					}
				}

				summary: {
					common: true
					description: """
						Similar to a histogram, a summary samples
						observations (usually things like request
						durations and response sizes). While it also
						provides a total count of observations and a
						sum of all observed values, it calculates
						configurable quantiles over a sliding time
						window.
						"""
					required: false
					warnings: []
					type: object: {
						examples: []
						options: {
							count: {
								description: "The total number of values contained within the summary."
								required:    true
								warnings: []
								type: uint: {
									examples: [54]
									unit: null
								}
							}
							quantiles: {
								description: "The set of observations."
								required:    true
								warnings: []
								type: array: items: type: object: {
									examples: []
									options: {
										value: {
											description: "The value of this quantile range."
											required:    true
											warnings: []
											// FIXME: making this float, as it should be, makes cue blow up
											type: uint: {
												// FIXME: Adding even empty examples makes cue blow up
												// examples: [2.1, 4.68, 23.02, 120.1]
												unit: null
											}
										}
										upper_limit: {
											description: "The upper limit for this quantile range, where 0 ≤ upper_limit ≤ 1."
											required:    true
											warnings: []
											// FIXME: making this float, as it should be, makes cue blow up
											type: uint: {
												// FIXME: Adding even empty examples makes cue blow up
												// examples: [0.1, 0.5, 0.75, 1.0]
												unit: null
											}
										}
									}
								}
							}
							sum: {
								description: "The sum of all values contained within the histogram."
								required:    true
								warnings: []
								type: float: {
									examples: [1.0, 10.0, 25.0, 100.0]
								}
							}
						}
					}
				}

				tags: {
					description: "The metric tags. Key/value pairs, nesting is not allowed."
					required:    true
					warnings: []
					type: object: {
						examples: [
							{
								"host":        "my.host.com"
								"instance_id": "abcd1234"
							},
						]
						options: {
							"*": {
								common:      true
								description: "Key/value pairs, nesting is not allowed."
								required:    false
								type: "*": {}
							}
						}
					}
				}

				"timestamp": {
					description: "The metric timestamp; when the metric was created."
					required:    true
					warnings: []
					type: timestamp: {}
				}
			}
		}
	}
}
