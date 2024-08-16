package metadata

base: components: sources: static_metrics: configuration: {
	interval_secs: {
		description: "The interval between metric emitting, in seconds."
		required:    false
		type: float: {
			default: 1.0
			unit:    "seconds"
		}
	}
	metrics: {
		description: "Tag configuration for the `internal_metrics` source."
		required:    false
		type: array: {
			default: []
			items: type: object: options: {
				kind: {
					description: "Kind of the static metric - either absolute or incremental"
					required:    true
					type: string: enum: {
						absolute:    "Absolute metric."
						incremental: "Incremental metric."
					}
				}
				name: {
					description: "Name of the static metric"
					required:    true
					type: string: {}
				}
				tags: {
					description: "Key-value pairs representing tags and their values to add to the metric."
					required:    false
					type: object: options: "*": {
						description: "An individual tag - value pair."
						required:    true
						type: string: {}
					}
				}
				value: {
					description: "\"Observed\" value of the static metric"
					required:    true
					type: object: options: {
						aggregated_histogram: {
							description: """
																			A set of observations which are counted into buckets.

																			It also contains the total count of all observations and their sum to allow calculating the mean.
																			"""
							required: true
							type: object: options: {
								buckets: {
									description: """
																							A histogram bucket.

																							The buckets within this histogram.
																							"""
									required: true
									type: array: items: type: object: options: {
										count: {
											description: "The number of values tracked in this bucket."
											required:    true
											type: uint: {}
										}
										upper_limit: {
											description: "The upper limit of values in the bucket."
											required:    true
											type: float: {}
										}
									}
								}
								count: {
									description: "The total number of observations contained within this histogram."
									required:    true
									type: uint: {}
								}
								sum: {
									description: "The sum of all observations contained within this histogram."
									required:    true
									type: float: {}
								}
							}
						}
						aggregated_summary: {
							description: """
																			A set of observations which are represented by quantiles.

																			Each quantile contains the upper value of the quantile (0 <= φ <= 1). It also contains the total count of all
																			observations and their sum to allow calculating the mean.
																			"""
							required: true
							type: object: options: {
								count: {
									description: "The total number of observations contained within this summary."
									required:    true
									type: uint: {}
								}
								quantiles: {
									description: """
																							A single quantile observation.

																							The quantiles measured from this summary.
																							"""
									required: true
									type: array: items: type: object: options: {
										quantile: {
											description: """
																														The value of the quantile.

																														This value must be between 0.0 and 1.0, inclusive.
																														"""
											required: true
											type: float: {}
										}
										value: {
											description: "The estimated value of the given quantile within the probability distribution."
											required:    true
											type: float: {}
										}
									}
								}
								sum: {
									description: "The sum of all observations contained within this histogram."
									required:    true
									type: float: {}
								}
							}
						}
						counter: {
							description: "A cumulative numerical value that can only increase or be reset to zero."
							required:    true
							type: object: options: value: {
								description: "The value of the counter."
								required:    true
								type: float: {}
							}
						}
						distribution: {
							description: "A set of observations without any aggregation or sampling."
							required:    true
							type: object: options: {
								samples: {
									description: "The observed values within this distribution."
									required:    true
									type: array: items: type: object: options: {
										rate: {
											description: "The rate at which the value was observed."
											required:    true
											type: uint: {}
										}
										value: {
											description: "The value of the observation."
											required:    true
											type: float: {}
										}
									}
								}
								statistic: {
									description: "The type of statistics to derive for this distribution."
									required:    true
									type: string: enum: {
										histogram: "A histogram representation."
										summary: """
																										Corresponds to Datadog's Distribution Metric
																										<https://docs.datadoghq.com/developers/metrics/types/?tab=distribution#definition>
																										"""
									}
								}
							}
						}
						gauge: {
							description: "A single numerical value that can arbitrarily go up and down."
							required:    true
							type: object: options: value: {
								description: "The value of the gauge."
								required:    true
								type: float: {}
							}
						}
						set: {
							description: "A set of (unordered) unique values for a key."
							required:    true
							type: object: options: values: {
								description: "The values in the set."
								required:    true
								type: array: items: type: string: {}
							}
						}
						sketch: {
							description: """
																			A data structure that can answer questions about the cumulative distribution of the contained samples in
																			space-efficient way.

																			Sketches represent the data in a way that queries over it have bounded error guarantees without needing to hold
																			every single sample in memory. They are also, typically, able to be merged with other sketches of the same type
																			such that client-side _and_ server-side aggregation can be accomplished without loss of accuracy in the queries.
																			"""
							required: true
							type: object: options: sketch: {
								description: "A generalized metrics sketch."
								required:    true
								type: object: options: AgentDDSketch: {
									description: """
																											[DDSketch][ddsketch] implementation based on the [Datadog Agent][ddagent].

																											While DDSketch has open-source implementations based on the white paper, the version used in
																											the Datadog Agent itself is subtly different. This version is suitable for sending directly
																											to Datadog's sketch ingest endpoint.

																											[ddsketch]: https://www.vldb.org/pvldb/vol12/p2195-masson.pdf
																											[ddagent]: https://github.com/DataDog/datadog-agent
																											"""
									required: true
									type: object: options: {
										avg: {
											description: "The average value of all observations within the sketch."
											required:    true
											type: float: {}
										}
										bins: {
											description: """
																															A split representation of sketch bins.

																															The bins within the sketch.
																															"""
											required: true
											type: object: options: {
												k: {
													description: "The bin keys."
													required:    true
													type: array: items: type: int: {}
												}
												n: {
													description: "The bin counts."
													required:    true
													type: array: items: type: uint: {}
												}
											}
										}
										count: {
											description: "The number of observations within the sketch."
											required:    true
											type: uint: {}
										}
										max: {
											description: "The maximum value of all observations within the sketch."
											required:    true
											type: float: {}
										}
										min: {
											description: "The minimum value of all observations within the sketch."
											required:    true
											type: float: {}
										}
										sum: {
											description: "The sum of all observations within the sketch."
											required:    true
											type: float: {}
										}
									}
								}
							}
						}
					}
				}
			}
		}
	}
	namespace: {
		description: "Overrides the default namespace for the metrics emitted by the source."
		required:    false
		type: string: default: "static"
	}
}
