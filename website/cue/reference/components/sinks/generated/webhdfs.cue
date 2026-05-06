package metadata

generated: components: sinks: webhdfs: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: """
				Controls whether or not end-to-end acknowledgements are enabled.

				When enabled for a sink, any source that supports end-to-end
				acknowledgements that is connected to that sink waits for events
				to be acknowledged by **all connected sinks** before acknowledging them at the source.

				Enabling or disabling acknowledgements at the sink level takes precedence over any global
				[`acknowledgements`][global_acks] configuration.

				[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
				"""
			required: false
			type: bool: {}
		}
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type: object: options: {
			max_bytes: {
				description: """
					The maximum size of a batch that is processed by a sink.

					This is based on the uncompressed size of the batched events, before they are
					serialized or compressed.
					"""
				required: false
				type: uint: {
					default: 10000000
					unit:    "bytes"
				}
			}
			max_events: {
				description: "The maximum size of a batch before it is flushed."
				required:    false
				type: uint: unit: "events"
			}
			timeout_secs: {
				description: "The maximum age of a batch before it is flushed."
				required:    false
				type: float: {
					default: 300.0
					unit:    "seconds"
				}
			}
		}
	}
	compression: {
		description: """
			Compression configuration.

			All compression algorithms use the default compression level unless otherwise specified.
			"""
		required: false
		type: string: {
			default: "gzip"
			enum: {
				gzip: """
					[Gzip][gzip] compression.

					[gzip]: https://www.gzip.org/
					"""
				none: "No compression."
				snappy: """
					[Snappy][snappy] compression.

					[snappy]: https://github.com/google/snappy/blob/main/docs/README.md
					"""
				zlib: """
					[Zlib][zlib] compression.

					[zlib]: https://zlib.net/
					"""
				zstd: """
					[Zstandard][zstd] compression.

					[zstd]: https://facebook.github.io/zstd/
					"""
			}
		}
	}
	endpoint: {
		description: """
			An HDFS cluster consists of a single NameNode, a master server that manages the file system namespace and regulates access to files by clients.

			The endpoint is the HDFS's web restful HTTP API endpoint.

			For more information, see the [HDFS Architecture][hdfs_arch] documentation.

			[hdfs_arch]: https://hadoop.apache.org/docs/r3.3.4/hadoop-project-dist/hadoop-hdfs/HdfsDesign.html#NameNode_and_DataNodes
			"""
		required: false
		type: string: {
			default: ""
			examples: ["http://127.0.0.1:9870"]
		}
	}
	prefix: {
		description: """
			A prefix to apply to all keys.

			Prefixes are useful for partitioning objects, such as by creating a blob key that
			stores blobs under a particular directory. If using a prefix for this purpose, it must end
			in `/` to act as a directory path. A trailing `/` is **not** automatically added.

			The final file path is in the format of `{root}/{prefix}{suffix}`.
			"""
		required: false
		type: string: {
			default: ""
			syntax:  "template"
		}
	}
	root: {
		description: """
			The root path for WebHDFS.

			Must be a valid directory.

			The final file path is in the format of `{root}/{prefix}{suffix}`.
			"""
		required: false
		type: string: default: ""
	}
}

generated: components: sinks: webhdfs: configuration: encoding: encodingBase & {
	type: object: options: codec: required: true
}
generated: components: sinks: webhdfs: configuration: framing: framingEncoderBase & {
	type: object: options: method: required: true
}
