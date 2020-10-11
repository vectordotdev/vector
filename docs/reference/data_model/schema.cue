package metadata

data_model: schema: {
	log: {
		common: true
		description: #"""
			A Vector log event is a structured representation of a
			point-in-time event. It contains an arbitrary set of
			fields that describe the event.

			A key tenet of Vector is to remain neutral in the way it
			processes data. This ensures Vector can support a
			variety of schemas without issue and it's why Vector's
			log data model does not require any specific fields.
			Instead, each Vector source will document it's output
			schema allowing you work with data in any shape.
			"""#
		required: false
		warnings: []
		type: object: {
			examples: []
			options: {
				"*": {
					common:      true
					description: "An arbitrary set of key/value pairs."
					required:    false
					type: "*": {}
				}
			}
		}
	}

	metric: {
		common: true
		description: #"""
			A Vector metric event represents a numerical operation
			performed on a time series. Operations offered are
			heavily inspired by the StatsD, Prometheus, and Datadog
			data models, and determine the schema of the metric
			structure within Vector.
			"""#
		required: false
		warnings: []
		type: object: {
			examples: []
			options: {
				counter: {
					common: true
					description: #"""
						A single value that can only be incremented
						or reset to zero value, it cannot be
						decremented.
						"""#
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
					description: #"""
						A distribution represents a distribution of
						sampled values. It is used with services
						that support global histograms.
						"""#
					required: false
					warnings: []
					type: object: {
						examples: []
						options: {
							sample_rates: {
								description: "The rate at which each individual value was sampled."
								required:    true
								warnings: []
								type: "[uint]": {
									examples: [[12, 43, 25]]
								}
							}
							values: {
								description: "The list of values contained within the distribution."
								required:    true
								warnings: []
								type: "[float]": {
									examples: [[12.0, 43.3, 25.2]]
								}
							}
						}
					}
				}

				gauge: {
					common: true
					description: #"""
						A gauge represents a point-in-time value
						that can increase and decrease. Vector's
						internal gauge type represents changes to
						that value. Gauges should be used to track
						fluctuations in values, like current memory
						or CPU usage.
						"""#
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
					description: #"""
						Also called a "timer". A histogram samples
						observations (usually things like request
						durations or response sizes) and counts them
						in configurable buckets. It also provides a
						sum of all observed values.
						"""#
					required: false
					warnings: []
					type: object: {
						examples: []
						options: {
							buckets: {
								description: "The buckets contained within this histogram."
								required:    true
								warnings: []
								type: "[uint]": {
									examples: [[1, 2, 5, 10, 25]]
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
							counts: {
								description: "The number of values contained within each bucket."
								required:    true
								warnings: []
								type: "[uint]": {
									examples: [[1, 10, 25, 100]]
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

				set: {
					common: true
					description: #"""
						A set represents an array of unique values.
						"""#
					required: false
					warnings: []
					type: object: {
						examples: []
						options: {
							values: {
								description: "The list of unique values."
								required:    true
								warnings: []
								type: "[string]": {
									examples: [["value1", "value2"]]
								}
							}
						}
					}
				}

				summary: {
					common: true
					description: #"""
						Similar to a histogram, a summary samples
						observations (usually things like request
						durations and response sizes). While it also
						provides a total count of observations and a
						sum of all observed values, it calculates
						configurable quantiles over a sliding time
						window.
						"""#
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
								description: "The quantiles contained within the summary, where 0 ≤ quantile ≤ 1."
								required:    true
								warnings: []
								type: "[float]": {
									examples: [[0.1, 0.5, 0.75, 1.0]]
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
							values: {
								description: "The values contained within the summary that align with the `quantiles`."
								required:    true
								warnings: []
								type: "[float]": {
									examples: [[2.1, 4.68, 23.02, 120.1]]
								}
							}
						}
					}
				}
			}
		}
	}
}
