package metadata

import "strings"

_values: {
	current_timestamp: "2020-10-10T17:07:36.452332Z"
	local_host:        "my-host.local"
	remote_host:       "34.33.222.212"
	instance:          "vector:9598"
	client_metadata: {
		subject: "CN=localhost,OU=Vector,O=Datadog,L=New York,ST=New York,C=US"
	}
}

// `#Any` allows for any value.
#Any: *_ | {[_=string]: #Any}

#Arch: "ARM64" | "ARMv7" | "x86_64"

// `#CompressionAlgorithm` specified data compression algorithm.
//
// * `none` - compression is not applied
// * `gzip` - gzip compression applied
#CompressionAlgorithm: "none" | "gzip" | "lz4" | "snappy" | "zstd" | "zlib"

#CompressionLevel: "none" | "fast" | "default" | "best" | >=0 & <=9

#Date: =~"^\\d{4}-\\d{2}-\\d{2}"

// `#DeliveryStatus` documents the delivery guarantee.
//
// * `at_least_once` - The event will be delivered at least once and
// could be delivered more than once.
// * `best_effort` - We will make a best effort to deliver the event,
// but the event is not guaranteed to be delivered.
#DeliveryStatus: "at_least_once" | "best_effort"

// `#DeploymentRoles` clarify when a component should be used under
// certain deployment contexts.
//
// * `daemon` - Vector is installed as a single process on the host.
// * `sidecar` - Vector is installed alongside each process it is
//   monitoring. Therefore, there might be multiple Vector processes
//   on the host.
// * `aggregator` - Vector receives data from one or more upstream
//   sources, typically over a network protocol.
#DeploymentRole: "aggregator" | "daemon" | "sidecar"

// `#DevelopmentStatus` documents the development status of the component.
//
// * `beta` - The component is early in its development cycle and the
// API and reliability are not settled.
// * `stable` - The component is production ready.
// * `deprecated` - The component will be removed in a future version.
// * `removed` - The component has been removed.
#DevelopmentStatus: "beta" | "stable" | "deprecated" | "removed"

#EncodingCodec: "json" | "logfmt" | "text" | "csv" | "native" | "native_json" | "avro" | "gelf"

#Endpoint: {
	description: string
	responses: [Code=string]: {
		description: string
	}
}

#Endpoints: [Path=string]: {
	DELETE?: #Endpoint
	GET?:    #Endpoint
	POST?:   #Endpoint
	PUT?:    #Endpoint
}

// `enum` restricts the value to a set of values.
//
//                enum: {
//                 json: "Encodes the data via application/json"
//                 text: "Encodes the data via text/plain"
//                }
#Enum: [Name=_]: string

#EnvVars: #Schema & {[Type=string]: {
	common:   true
	required: false
}}

#Event: {
	{log?: #LogEvent} |
	{metric?: #MetricEvent}
}

// `#EventType` represents one of Vector's supported event types.
//
// * `log` - log event
// * `metric` - metric event
#EventType: "log" | "metric"

#Fields: [Name=string]: #Fields | *_

#Interface: {
	{binary: #InterfaceBinary} |
	{ffi: {}} |
	{file_system: #InterfaceFileSystem} |
	{socket: #InterfaceSocket} |
	{stdin: {}} |
	{stdout: {}}
}

#InterfaceBinary: {
	name:         string
	permissions?: #Permissions
}

#InterfaceFileSystem: {
	directory: string
}

#InterfaceSocket: {
	api?: {
		title: string
		url:   string
	}
	direction: "incoming" | "outgoing"

	if direction == "outgoing" {
		network_hops?: uint8
		permissions?:  #Permissions
	}

	if direction == "incoming" {
		port: uint16
	}

	protocols: [#Protocol, ...#Protocol]
	socket?: string
	ssl:     "disabled" | "required" | "optional"
}

#HowItWorks: [Name=string]: {
	#Subsection: {
		title: string
		body:  string
	}

	name:  Name
	title: string
	body:  string | null
	svg?:  string
	sub_sections?: [#Subsection, ...#Subsection]
}

#LogEvent: {
	...
}

#Map: [string]: string

#MetricEvent: {
	kind:       "incremental" | "absolute"
	name:       string
	namespace?: string
	tags: [Name=string]: string
	timestamp?: string
	{counter: #MetricEventCounter} |
	{distribution: #MetricEventDistribution} |
	{gauge: #MetricEventGauge} |
	{histogram: #MetricEventHistogram} |
	{set: #MetricEventSet} |
	{summary: #MetricEventSummary}
}

#MetricEventCounter: {
	value: float
}

#MetricEventDistribution: {
	samples: [#DistributionSample, ...#DistributionSample]
	statistic: "histogram" | "summary"
}

#DistributionSample: {
	value: float
	rate:  uint
}

#MetricEventGauge: {
	value: float
}

#MetricEventHistogram: {
	buckets: [#HistogramBucket, ...#HistogramBucket]
	count: uint
	sum:   float
}

#HistogramBucket: {
	upper_limit: float
	count:       uint
}

#MetricEventSet: {
	values: [string, ...string]
}

#MetricEventSummary: {
	quantiles: [#SummaryQuantile, ...#SummaryQuantile]
	count: int
	sum:   float
}

#SummaryQuantile: {
	upper_limit: float
	value:       float
}

#MetricTags: [Name=string]: {
	name:        Name
	default?:    string
	description: string
	enum?:       #Enum
	examples?: [string, ...string]
	required: bool
}

#MetricType: "counter" | "distribution" | "gauge" | "histogram" | "summary"

#Object: {[_=string]: #Any}

#OperatingSystemFamily: "Linux" | "macOS" | "Windows"

#Permissions: {
	unix: {
		group: string
	}
}

#Protocol: "http" | "tcp" | "udp" | "unix" | "unix_datagram" | "unix_stream"

#Service: {
	// `description` describes the components with a single paragraph.
	// It is used for SEO purposes and should be full of relevant keywords.
	description?: =~"[.]$"

	name:     string
	thing:    string
	url:      string
	versions: string | null

	setup?: #SetupSteps

	connect_to?: [_=string]: {
		logs?: {
			description?: string
			setup:        #SetupSteps
		}

		metrics?: {
			description?: string
			setup:        #SetupSteps
		}
	}
}

#SetupStep: {
	title:        string
	description?: string
	notes?: [...string]

	detour?: {
		url: string
	}

	vector?: {
		configure: #Object
	}

	if detour == _|_ && vector == _|_ {
		description: string
	}
}

#SetupSteps: [#SetupStep, ...#SetupStep]

#Schema: [Name=string]: #SchemaField & {name: Name}

#SchemaField: {
	// `category` allows you to group options into categories.
	//
	// For example, all of the `*_key` options might be grouped under the
	// "Context" category to make generated configuration examples easier to
	// read.
	category?: string

	if type.object != _|_ {
		category: strings.ToTitle(name)
	}

	// `description` describes the option in a succinct fashion. Usually 1 to
	// 2 sentences.
	description: string

	// `groups` groups options into categories.
	//
	// For example, the `influxdb_logs` sink supports both v1 and v2 of Influxdb
	// and relevant options are placed in those groups.
	groups?: [string, ...string]

	// `name` sets the name for this option. It is automatically set for you
	// via the key you use.
	name: string

	// `deprecated` sets if the given field has been deprecated.
	deprecated: bool | *false

	if deprecated {
		// If a field has been deprecated we can optionally set a deprecated
		// message to be displayed.
		deprecated_message?: string
	}

	// `relevant_when` clarifies when an option is relevant.
	//
	// For example, if an option depends on the value of another option you can
	// specify that here. We accept a string to allow for the expression of
	// complex requirements.
	//
	//              relevant_when: 'strategy = "fingerprint"'
	//              relevant_when: 'strategy = "fingerprint" or "inode"'
	relevant_when?: string

	// `required` requires the option to be set.
	required: bool

	// `warnings` warn the user about some aspects of the option.
	//
	// For example, the `tls.verify_hostname` option has a warning about
	// reduced security if the option is disabled.
	warnings: [...string] | *[]

	if !required {
		// `common` specifies that the option is commonly used. It will bring the
		// option to the top of the documents, surfacing it from other
		// less common, options.
		common?: bool
	}

	// `sort` sorts the option, otherwise options will be sorted alphabetically.
	sort?: int8

	// `types` sets the option's value type. External tagging is used since
	// each type has its own set of fields.
	type: #Type & {_args: "required": required}
}

#TargetTriples: {
	"aarch64-unknown-linux-gnu":      bool | *true
	"aarch64-unknown-linux-musl":     bool | *true
	"armv7-unknown-linux-gnueabihf":  bool | *true
	"armv7-unknown-linux-musleabihf": bool | *true
	"x86_64-apple-darwin":            bool | *true
	"x86_64-pc-windows-msv":          bool | *true
	"x86_64-unknown-linux-gnu":       bool | *true
	"x86_64-unknown-linux-musl":      bool | *true
}

#Timestamp: =~"^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}.\\d{6}Z"

#Type: {
	_args: {
		name:     !=""
		arrays:   true
		required: bool
	}
	let Args = _args

	// `*` represents a wildcard type.
	//
	// For example, the `sinks.http.headers.*` option allows for arbitrary
	// key/value pairs.
	{"array": #TypeArray & {_args: required: Args.required}} |
	{"condition": #TypeCondition & {_args: required: Args.required}} |
	#TypePrimitive
}

#TypeCondition: {
	_args: {
		required: bool
	}
	let Args = _args
	required: Args.required

	#Syntax: {
		name:        !=""
		description: !=""
		example:     string | null
	}

	#ConditionExample: {
		title:    !=""
		name:     "vrl" | "datadog_search" | "is_log" | "is_metric" | "is_trace"
		example:  !=""
		vrl_only: bool | *false
	}

	syntaxes: [#Syntax, ...#Syntax] & [
		{
			name:        "vrl"
			description: """
				A [Vector Remap Language](\(urls.vrl_reference)) (VRL) [Boolean
				expression](\(urls.vrl_boolean_expression)).
				"""
			example:     #".status_code != 200 && !includes(["info", "debug"], .severity)"#
		},
		{
			name:        "datadog_search"
			description: "A [Datadog Search](\(urls.datadog_search_syntax)) query string."
			example:     #"*stack"#
		},
		{
			name:        "is_log"
			description: "Whether the incoming event is a log."
			example:     null
		},
		{
			name:        "is_metric"
			description: "Whether the incoming event is a metric."
			example:     null
		},
		{
			name:        "is_trace"
			description: "Whether the incoming event is a trace."
			example:     null
		},
	]

	options: {
		source: {
			description: "The text of the condition. The syntax of the condition depends on the value of `type`."
		}

		type: {
			description: """
				The type of condition to supply. See **Available syntaxes** below for a list of available types for this
				transform.
				"""
		}
	}

	shorthand_explainer: {
		title:       "Shorthand for VRL"
		description: """
			If you opt for the [`vrl`](\(urls.vrl_reference)) syntax for this condition, you can set the condition
			as a string via the `condition` parameter, without needing to specify both a `source` and a `type`. The
			table below shows some examples:

			Config format | Example
			:-------------|:-------
			[YAML](\(urls.yaml)) | `condition: .status == 200`
			[TOML](\(urls.toml)) | `condition = ".status == 200"`
			[JSON](\(urls.json)) | `"condition": ".status == 200"`
			"""
	}

	condition_examples: [#ConditionExample, ...#ConditionExample] & [
		{
			title:   "Standard VRL"
			name:    "vrl"
			example: ".status == 500"
		},
		{
			title:   "Datadog Search"
			name:    "datadog_search"
			example: "*stack"
		},
		{
			title:    "VRL shorthand"
			name:     "vrl"
			example:  ".status == 500"
			vrl_only: true
		},
	]
}

#TypePrimitive: {
	_args: {
		arrays:   true
		required: bool
	}
	let Args = _args

	// `*` represents a wildcard type.
	//
	// For example, the `sinks.http.headers.*` option allows for arbitrary
	// key/value pairs.
	"*"?: {}
	"bool"?: #TypeBool & {_args: required: Args.required}
	"float"?: #TypeFloat & {_args: required: Args.required}
	"object"?: #TypeObject & {_args: required: Args.required}
	"string"?: #TypeString & {_args: required: Args.required}
	"ascii_char"?: #TypeAsciiChar & {_args: required: Args.required}
	"timestamp"?: #TypeTimestamp & {_args: required: Args.required}
	"uint"?: #TypeUint & {_args: required: Args.required}
}

#TypeArray: {
	_args: required: bool
	_type: items: type: string
	let Args = _args
	let Type = _type

	if !Args.required {
		// `default` sets the default value.
		default: [...] | *null
	}

	examples?: [...[...Type.items.type]]

	// Set `required` to `true` to force disable defaults. Defaults should
	// be specified on the array level and not the type level.
	items: type: #TypePrimitive & {_args: required: true}
}

#TypeBool: {
	_args: required: bool
	let Args = _args

	if !Args.required {
		// `default` sets the default value.
		default: bool | *null
	}
}

#TypeFloat: {
	_args: required: bool
	let Args = _args

	if !Args.required {
		// `default` sets the default value.
		default: float | *null
	}

	// `examples` clarify values through examples. This should be used
	// when examples cannot be derived from the `default` or `enum`
	// options.
	examples?: [float, ...float]

	// `unit` clarifies the value's unit. While this should be included
	// as the suffix in the name, this helps to explicitly clarify that.
	unit?: #Unit | null
}

#TypeObject: {
	// `examples` clarify values through examples. This should be used
	// when examples cannot be derived from the `default` or `enum`
	// options.
	examples: [#Object, ...#Object] | *[]

	// `options` represent the child options for this option.
	options: #Schema
}

#TypeString: {
	_args: required: bool
	let Args = _args

	if !Args.required {
		// `default` sets the default value.
		default: string | *null
	}

	// `enum` restricts the value to a set of values.
	//
	//      enum: {
	//        json: "Encodes the data via application/json"
	//        text: "Encodes the data via text/plain"
	//      }
	enum?: #Enum

	examples?: [...string]

	if Args.required && enum != _|_ {
		// `examples` demonstrates example values. This should be used when
		// examples cannot be derived from the `default` or `enum` options.
		examples: [string, ...string] | *[
			for k, v in enum {
				k
			},
		]
	}

	syntax: *"literal" | "file_system_path" | "field_path" | "template" | "regex" | "remap_program" | "strftime"
}

#TypeAsciiChar: {
	_args: required: bool
	let Args = _args

	if !Args.required {
		// `default` sets the default value.
		default: string | *null
	}

	examples?: [string, ...string]
}

#TypeTimestamp: {
	_args: required: bool
	let Args = _args

	if !Args.required {
		// `default` sets the default value.
		default: #Timestamp | *null
	}

	// `examples` clarify values through examples. This should be used
	// when examples cannot be derived from the `default` or `enum`
	// options.
	examples: [_values.current_timestamp]
}

#TypeUint: {
	_args: required: bool
	let Args = _args

	if !Args.required {
		// `default` sets the default value.
		default: uint | *null
	}

	// `examples` clarify values through examples. This should be used
	// when examples cannot be derived from the `default` or `enum`
	// options.
	examples?: [uint, ...uint]

	// `unit` clarifies the value's unit. While this should be included
	// as the suffix in the name, this helps to explicitly clarify that.
	unit?: #Unit | null
}

#Unit: "bytes" | "events" | "milliseconds" | "nanoseconds" | "requests" | "seconds" | "lines" | "concurrency" | "connections" | "tasks" | "retries"

administration: _
components:     _
configuration:  _
data_model:     _
glossary:       _
process:        _
releases:       _
remap:          _

// Reusable info
_coercing_fields: """
	Key/value pairs representing mapped log field names and types. This is used to
	coerce log fields from strings into their proper types. The available types are
	listed in the **Types** list below.

	Timestamp coercions need to be prefaced with `timestamp|`, for example `\"timestamp|%F\"`.
	Timestamp specifiers can use either of the following:

	1. One of the built-in-formats listed in the **Timestamp Formats** table below.
	2. The [time format specifiers](\(urls.chrono_time_formats)) from Rust's
	   `chrono` library.

	**Types**

	* `bool`
	* `string`
	* `float`
	* `integer`
	* `date`
	* `timestamp` (see the table below for formats)

	**Timestamp Formats**

	Format | Description | Example
	:------|:------------|:-------
	`%F %T` | `YYYY-MM-DD HH:MM:SS` | `2020-12-01 02:37:54`
	`%v %T` | `DD-Mmm-YYYY HH:MM:SS` | `01-Dec-2020 02:37:54`
	`%FT%T` | [ISO 8601](\(urls.iso_8601))/[RFC 3339](\(urls.rfc_3339)) format without time zone | `2020-12-01T02:37:54`
	`%a, %d %b %Y %T` | [RFC 822](\(urls.rfc_822))/[2822](\(urls.rfc_2822)) without time zone | `Tue, 01 Dec 2020 02:37:54`
	`%a %d %b %T %Y` | [`date`](\(urls.date)) command output without time zone | `Tue 01 Dec 02:37:54 2020`
	`%a %b %e %T %Y` | [ctime](\(urls.ctime)) format | `Tue Dec  1 02:37:54 2020`
	`%s` | [UNIX](\(urls.unix_timestamp)) timestamp | `1606790274`
	`%FT%TZ` | [ISO 8601](\(urls.iso_8601))/[RFC 3339](\(urls.rfc_3339)) UTC | `2020-12-01T09:37:54Z`
	`%+` | [ISO 8601](\(urls.iso_8601))/[RFC 3339](\(urls.rfc_3339)) UTC with time zone | `2020-12-01T02:37:54-07:00`
	`%a %d %b %T %Z %Y` | [`date`](\(urls.date)) command output with time zone | `Tue 01 Dec 02:37:54 PST 2020`
	`%a %d %b %T %z %Y`| [`date`](\(urls.date)) command output with numeric time zone | `Tue 01 Dec 02:37:54 -0700 2020`
	`%a %d %b %T %#z %Y` | [`date`](\(urls.date)) command output with numeric time zone (minutes can be missing or present) | `Tue 01 Dec 02:37:54 -07 2020`

	**Note**: the examples in this table are for 54 seconds after 2:37 am on December 1st, 2020 in Pacific Standard Time.
	"""
