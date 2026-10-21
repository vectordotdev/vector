package metadata

_schemaDefinitions: "codecs::encoding::config::EncodingConfig": object: options: {
	avro: {
		description:   "Apache Avro-specific encoder options."
		relevant_when: "codec = \"avro\""
		required:      true
		type:          _schemaDefinitions["codecs::encoding::format::avro::AvroSerializerOptions"]
	}
	cef: {
		description:   "The CEF Serializer Options."
		relevant_when: "codec = \"cef\""
		required:      true
		type:          _schemaDefinitions["codecs::encoding::format::cef::CefSerializerOptions"]
	}
	codec: {
		description: "The codec to use for encoding events."
		required:    true
		type: string: enum: {
			avro: """
				Encodes an event as an [Apache Avro][apache_avro] message.

				[apache_avro]: https://avro.apache.org/
				"""
			cef: "Encodes an event as a CEF (Common Event Format) formatted message."
			csv: """
				Encodes an event as a CSV message.

				This codec must be configured with fields to encode.
				"""
			gelf: """
				Encodes an event as a [GELF][gelf] message.

				This codec is experimental for the following reason:

				The GELF specification is more strict than the actual Graylog receiver.
				Vector's encoder currently adheres more strictly to the GELF spec, with
				the exception that some characters such as `@`  are allowed in field names.

				Other GELF codecs, such as Loki's, use a [Go SDK][implementation] that is maintained
				by Graylog and is much more relaxed than the GELF spec.

				Going forward, Vector will use that [Go SDK][implementation] as the reference implementation, which means
				the codec might continue to relax the enforcement of the specification.

				[gelf]: https://docs.graylog.org/docs/gelf
				[implementation]: https://github.com/Graylog2/go-gelf/blob/v2/gelf/reader.go
				"""
			json: """
				Encodes an event as [JSON][json].

				[json]: https://www.json.org/
				"""
			logfmt: """
				Encodes an event as a [logfmt][logfmt] message.

				[logfmt]: https://brandur.org/logfmt
				"""
			native: """
				Encodes an event in the [native Protocol Buffers format][vector_native_protobuf].

				This codec is **[experimental][experimental]**.

				[vector_native_protobuf]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/proto/event.proto
				[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
				"""
			native_json: """
				Encodes an event in the [native JSON format][vector_native_json].

				This codec is **[experimental][experimental]**.

				[vector_native_json]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
				[experimental]: https://vector.dev/highlights/2022-03-31-native-event-codecs
				"""
			otlp: """
				Encodes an event in the [OTLP (OpenTelemetry Protocol)][otlp] format.

				This codec uses protobuf encoding, which is the recommended format for OTLP.
				The output is suitable for sending to OTLP-compatible endpoints with
				`content-type: application/x-protobuf`.

				[otlp]: https://opentelemetry.io/docs/specs/otlp/
				"""
			protobuf: """
				Encodes an event as a [Protobuf][protobuf] message.

				[protobuf]: https://protobuf.dev/
				"""
			raw_message: """
				No encoding.

				This encoding uses the `message` field of a log event.

				Be careful if you are modifying your log events (for example, by using a `remap`
				transform) and removing the message field while doing additional parsing on it, as this
				could lead to the encoding emitting empty strings for the given event.
				"""
			syslog: """
				Syslog encoding
				RFC 3164 and 5424 are supported
				"""
			text: """
				Plain text encoding.

				This encoding uses the `message` field of a log event. For metrics, it uses an
				encoding that resembles the Prometheus export format.

				Be careful if you are modifying your log events (for example, by using a `remap`
				transform) and removing the message field while doing additional parsing on it, as this
				could lead to the encoding emitting empty strings for the given event.
				"""
		}
	}
	csv: {
		description:   "The CSV Serializer Options."
		relevant_when: "codec = \"csv\""
		required:      true
		type:          _schemaDefinitions["codecs::encoding::format::csv::CsvSerializerOptions"]
	}
	except_fields: {
		description: "List of fields that are excluded from the encoded event."
		required:    false
		type: array: items: type: string: {}
	}
	gelf: {
		description:   "The GELF Serializer Options."
		relevant_when: "codec = \"gelf\""
		required:      false
		type:          _schemaDefinitions["codecs::encoding::format::gelf::GelfSerializerOptions"]
	}
	json: {
		description:   "Options for the JsonSerializer."
		relevant_when: "codec = \"json\""
		required:      false
		type:          _schemaDefinitions["codecs::encoding::format::json::JsonSerializerOptions"]
	}
	metric_tag_values: {
		description: """
			Controls how metric tag values are encoded.

			When set to `single`, only the last non-bare value of tags are displayed with the
			metric. When set to `full`, all metric tags are exposed as separate assignments.
			When set to `auto`, tag values are encoded using their underlying shape.
			"""
		relevant_when: "codec = \"json\" or codec = \"text\""
		required:      false
		type: string: {
			default: "single"
			enum: {
				auto: """
					Tag values are exposed using their underlying shape: single-value tags as strings,
					multi-value tags as arrays. A length-1 array round-trips as a scalar; use `Full` to
					force array shape.
					"""
				full: "All tags are exposed as arrays of either string or null values."
				single: """
					Tag values are exposed as single strings, the same as they were before this config
					option. Tags with multiple values show the last assigned value, and null values
					are ignored.
					"""
			}
		}
	}
	only_fields: {
		description: "List of fields that are included in the encoded event."
		required:    false
		type: array: items: type: string: {}
	}
	protobuf: {
		description:   "Options for the Protobuf serializer."
		relevant_when: "codec = \"protobuf\""
		required:      true
		type:          _schemaDefinitions["codecs::encoding::format::protobuf::ProtobufSerializerOptions"]
	}
	syslog: {
		description:   "Options for the Syslog serializer."
		relevant_when: "codec = \"syslog\""
		required:      false
		type:          _schemaDefinitions["codecs::encoding::format::syslog::SyslogSerializerOptions"]
	}
	timestamp_format: {
		description: "Format used for timestamp fields."
		required:    false
		type: string: enum: {
			rfc3339:    "Represent the timestamp as a RFC 3339 timestamp."
			unix:       "Represent the timestamp as a Unix timestamp."
			unix_float: "Represent the timestamp as a Unix timestamp in floating point."
			unix_ms:    "Represent the timestamp as a Unix timestamp in milliseconds."
			unix_ns:    "Represent the timestamp as a Unix timestamp in nanoseconds."
			unix_us:    "Represent the timestamp as a Unix timestamp in microseconds."
		}
	}
}
