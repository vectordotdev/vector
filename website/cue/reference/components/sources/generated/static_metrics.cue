package metadata

generated: components: sources: static_metrics: configuration: {
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
						native_histogram: {
							description: """
																			A Prometheus-style native (exponential) histogram.

																			Native histograms use exponential bucket boundaries determined by a `schema` parameter, allowing for high
																			resolution at low cost. Unlike `AggregatedHistogram` which uses fixed bucket boundaries, native histograms use
																			sparse buckets indexed by integer keys, where adjacent buckets grow by a factor of `2^(2^-schema)`.

																			See <https://prometheus.io/docs/specs/native_histograms/> for details.
																			"""
							required: true
							type: object: options: {
								count: {
									description: """
																							The total number of observations.

																							May be a float to support gauge histograms where resets can cause fractional counts.
																							"""
									required: true
									type: object: options: {
										float: {
											description: "Floating-point count value."
											required:    true
											type: float: {}
										}
										integer: {
											description: "Integer count value."
											required:    true
											type: uint: {}
										}
									}
								}
								negative_buckets: {
									description: """
																							Bucket values for negative buckets.

																							For integer counts, these are deltas from the previous bucket (first is absolute). For float counts, these
																							are absolute values. The interpretation depends on the `count` type.
																							"""
									required: true
									type: object: options: {
										float_counts: {
											description: "Absolute floating-point bucket counts."
											required:    true
											type: array: items: type: float: {}
										}
										integer_deltas: {
											description: """
																											Delta-encoded integer bucket counts.

																											The first value is the absolute count of the first bucket; subsequent values are the delta from the previous
																											bucket's count.
																											"""
											required: true
											type: array: items: type: int: {}
										}
									}
								}
								negative_spans: {
									description: """
																							A span of consecutive populated buckets in a native histogram.

																							Spans of populated negative buckets.
																							"""
									required: true
									type: array: items: type: object: options: {
										length: {
											description: "Number of consecutive buckets in this span."
											required:    true
											type: uint: {}
										}
										offset: {
											description: "Gap in bucket indices from the previous span (or from zero for the first span)."
											required:    true
											type: int: {}
										}
									}
								}
								positive_buckets: {
									description: """
																							Bucket values for positive buckets.

																							For integer counts, these are deltas from the previous bucket (first is absolute). For float counts, these
																							are absolute values. The interpretation depends on the `count` type.
																							"""
									required: true
									type: object: options: {
										float_counts: {
											description: "Absolute floating-point bucket counts."
											required:    true
											type: array: items: type: float: {}
										}
										integer_deltas: {
											description: """
																											Delta-encoded integer bucket counts.

																											The first value is the absolute count of the first bucket; subsequent values are the delta from the previous
																											bucket's count.
																											"""
											required: true
											type: array: items: type: int: {}
										}
									}
								}
								positive_spans: {
									description: """
																							A span of consecutive populated buckets in a native histogram.

																							Spans of populated positive buckets.
																							"""
									required: true
									type: array: items: type: object: options: {
										length: {
											description: "Number of consecutive buckets in this span."
											required:    true
											type: uint: {}
										}
										offset: {
											description: "Gap in bucket indices from the previous span (or from zero for the first span)."
											required:    true
											type: int: {}
										}
									}
								}
								reset_hint: {
									description: "Hint about whether this represents a counter reset."
									required:    true
									type: string: enum: {
										gauge:   "This histogram is a gauge histogram (no reset semantics)."
										no:      "This histogram is known not to be the first after a reset."
										unknown: "No hint; receiver should detect resets from the data."
										yes:     "This histogram is the first after a reset (or the very first observation)."
									}
								}
								schema: {
									description: """
																							The resolution parameter.

																							Valid values are from -4 to 8 for standard exponential schemas. Higher values give finer resolution.
																							Bucket boundaries are at `(2^(2^-schema))^n` for positive buckets.
																							"""
									required: true
									type: int: {}
								}
								sum: {
									description: "The sum of all observations."
									required:    true
									type: float: {}
								}
								zero_count: {
									description: "Count of observations in the zero bucket."
									required:    true
									type: object: options: {
										float: {
											description: "Floating-point count value."
											required:    true
											type: float: {}
										}
										integer: {
											description: "Integer count value."
											required:    true
											type: uint: {}
										}
									}
								}
								zero_threshold: {
									description: """
																							The width of the "zero bucket".

																							Observations in `[-zero_threshold, zero_threshold]` are counted in the zero bucket rather than in positive
																							or negative exponential buckets.
																							"""
									required: true
									type: float: {}
								}
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

																											While `DDSketch` has open-source implementations based on the white paper, the version used in
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
											description: "The bins within the sketch."
											required:    true
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
