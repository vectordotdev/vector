package metadata

generated: components: sinks: webhdfs: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::util::batch::BulkSizeBasedDefaultBatchSettings>"]
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
	dangerously_allow_unconfined_template_resolution: {
		description: """
			Disable all template confinement checks for this sink.

			**DANGEROUS — disables a security control.**

			Bypasses both startup validation and runtime confinement for every
			templated field on this sink. When enabled, a log producer that
			controls any field used in a template can write to arbitrary keys,
			paths, or routing destinations. This flag is a full opt-out: it
			disables confinement even for templates that have a usable static
			prefix.
			"""
		required: false
		type: bool: default: false
	}
	encoding: {
		description: """
			Encoding configuration.
			Configures how events are encoded into raw bytes.
			The selected encoding also determines which input types (logs, metrics, traces) are supported.
			"""
		required: true
		type:     _schemaDefinitions["codecs::encoding::config::EncodingConfig"]
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
			default: "http://127.0.0.1:9870/"
			examples: ["http://127.0.0.1:9870"]
		}
	}
	framing: {
		description: "Framing configuration."
		required:    false
		type:        _schemaDefinitions["codecs::encoding::framing::framer::FramingConfig"]
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
