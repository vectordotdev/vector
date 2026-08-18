package metadata

components: sinks: file: {
	title: "File"

	classes: {
		delivery: "at_least_once"

		development:   "stable"
		egress_method: "stream"
		service_providers: []
		stateful: false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10000000
				timeout_secs: 300.0
			}
			compression: {
				enabled: true
				default: "none"
				algorithms: ["none", "gzip", "zstd"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: {
					enabled: true
					framing: true
					enum: ["json", "text"]
				}
			}
			request: enabled: false
			tls: enabled:     false
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: generated.components.sinks.file.configuration

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
		traces: true
	}

	how_it_works: {
		dir_and_file_creation: {
			title: "File & Directory Creation"
			body: """
				Vector will attempt to create the entire directory structure
				and the file when emitting events to the file sink. This
				requires that the Vector agent have the correct permissions
				to create and write to files in the specified directories.
				"""
		}

		durability: {
			title: "Durability of Created Files"
			body: """
				Vector makes no attempt to ensure the files output by
				this sink are durably written to disk by using any of
				the "sync" write modes. As such, this sink only
				ensures that the operating system does not generate an
				error, it does not wait until the data is written to
				disk before acknowledging the events.
				"""
		}

		parquet: {
			title: "Apache Parquet Encoding"
			body: """
				By default, the file sink writes events one at a time
				using the configured `encoding` and `framing`. Setting
				`batch_encoding.codec` to `parquet` switches the sink to
				batch events together and encode them as columnar
				[Apache Parquet](https://parquet.apache.org/) files. The
				`batch` option controls how events are grouped into
				batches when `batch_encoding` is set.

				Because columnar files cannot be appended to, each batch
				is written to a distinct file: a time-ordered UUID (v7) is
				inserted into the rendered `path` before the file
				extension so that successive batches do not overwrite one
				another. The columnar format handles its own internal
				compression, so the top-level `compression` setting is
				ignored when `batch_encoding` is set.
				"""
		}
	}

	telemetry: metrics: {
		open_files: components.sources.internal_metrics.output.metrics.open_files
	}
}
