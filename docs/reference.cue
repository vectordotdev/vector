// Root
//
// The root file defines the schema for all of Vector's reference metadata.
// It does not include boilerplate or domain specific policies.

package metadata

import (
	"strings"
)

_values: {
	current_timestamp: "2020-10-10T17:07:36.452332Z"
	local_host:        "my-host.local"
	remote_host:       "34.33.222.212"
}

// `#Any` allows for any value.
#Any: _ | {[_=string]: #Any}

// `#Classes` represent various `#Components` classifications.
#Classes: {
	_args: kind: string
	let Args = _args

	// `commonly_used` specifies if the component is commonly used or not.
	// Setting this to `true` will surface the component from other,
	// less commonly used, components.
	commonly_used: bool

	if Args.kind == "source" || Args.kind == "sink" {
		delivery: #DeliveryStatus
	}

	if Args.kind == "source" {
		// `deployment_roles` clarify when the component should be used under
		// different deployment contexts.
		deployment_roles: [#DeploymentRole, ...]
	}
	development: #DevelopmentStatus

	// `egress_method` documents how the component outputs events.
	egress_method: #EgressMethod

	if Args.kind == "sink" {
		// `service_providers` specify the service providers that support
		// and host this service. This helps users find relevant sinks.
		//
		// For example, "AWS" is a service provider for many services, and
		// a user on AWS can use this to filter for AWS supported
		// components.
		service_providers: [...string] | *[]
	}
}

#Commit: {
	author:           string
	breaking_change:  bool
	date:             #Date
	description:      string
	deletions_count:  uint
	files_count:      uint
	insertions_count: uint
	pr_number:        uint | null
	scopes:           [string, ...] | []
	sha:              #CommitSha
	type:             "chore" | "docs" | "enhancement" | "feat" | "fix" | "perf" | "status"
}

#CommitSha: =~"^[a-z0-9]{40}$"

// `#ComponentKind` represent the kind of component.
#ComponentKind: "sink" | "source" | "transform"

// `#Components` are any transform, source, or sink.
#Components: [Type=string]: {
	// `kind` specified the component kind. This is set automatically.
	kind: #ComponentKind
	let Kind = kind

	configuration: #Schema

	// `description` describes the components with a single paragraph.
	// It is used for SEO purposes and should be full of relevant keywords.
	description?: =~"[.]$"

	env_vars: #EnvVars

	// `type` is the component identifier. This is set automatically.
	type: Type

	// `classes` represent the various classifications for this component
	classes: #Classes & {_args: kind: Kind}

	// `examples` demonstrates various ways to use the component using an
	// input, output, and example configuration.
	examples: [
		...close({
			title:    string
			context?: string
			"configuration": {
				for k, v in configuration {
					"\( k )"?: _ | *null
				}
			}

			if Kind == "source" {
				input: string
			}

			if Kind != "source" {
				input: #Event | [#Event, ...]
			}

			if Kind == "sink" {
				output: string
			}

			if Kind != "sink" {
				output: #Event | [#Event, ...] | null
			}

			notes?: string
		}),
	]

	// `features` describes the various supported features of the component.
	// Setting these helps to reduce boilerplate.
	//
	// For example, the `tls` feature will automatically add the appropriate
	// `tls` options and `how_it_works` sections.
	features: #Features & {_args: {egress_method: classes.egress_method, kind: Kind}}

	// `how_it_works` contain sections that further describe the component's
	// behavior. This is like a mini-manual for the component and should help
	// answer any obvious questions the user might have. Options can link
	// to these sections for deeper explanations of behavior.
	how_it_works: #HowItWorks

	if Kind != "source" {
		input: #Input
	}

	if Kind != "sink" {
		// `output` documents output of the component. This is very important
		// as it communicate which events and fields are emitted.
		output: #Output
	}

	// `support` communicates the varying levels of support of the component.
	support: #Support & {_args: kind: Kind}

	// `title` is the human friendly title for the component.
	//
	// For example, the `http` sink has a `HTTP` title.
	title: string
}

// `#CompressionAlgorithm` specified data compression algorithm.
//
// * `none` - compression is not applied
// * `gzip` - gzip compression applied
#CompressionAlgorithm: "none" | "gzip"

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
// * `service` - Vector receives data from one or more upstream
//   sources, typically over a network protocol.
#DeploymentRole: "aggregator" | "daemon" | "sidecar"

// `#DevelopmentStatus` documents the development status of the component.
//
// * `beta` - The component is early in it's development cylce and the
// API and reliability are not settled.
// * `stable` - The component is production ready.
// * `deprecated` - The component will be removed in a future version.
#DevelopmentStatus: "beta" | "stable" | "deprecated"

// `#EgressMethod` specified how a component outputs events.
//
// * `batch` - one or more events at a time
// * `stream` - one event at a time
#EgressMethod: "batch" | "expose" | "stream"

#EncodingCodec: "json" | "ndjson" | "text"

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
	type: string: default: null
}}

#Event: {
	close({log: #LogEvent}) |
	close({metric: #MetricEvent})
}

// `#EventType` represents one of Vector's supported event types.
//
// * `log` - log event
// * `metric` - metric event
#EventType: "log" | "metric"

#Fields: [Name=string]: #Fields | _

#Interface: {
	close({binary: #InterfaceBinary}) |
	close({ffi: close({})}) |
	close({file_system: #InterfaceFileSystem}) |
	close({socket: #InterfaceSocket}) |
	close({stdin: close({})}) |
	close({stdout: close({})})
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

	protocols: [#Protocol, ...]
	socket?: string
	ssl:     "disabled" | "required" | "optional"
}

#Features: {
	_args: {
		egress_method: string
		kind:          string
	}
	let Args = _args

	if Args.kind == "source" {
		collect?:  #FeaturesCollect
		generate?: #FeaturesGenerate
		multiline: #FeaturesMultiline
		receive?:  #FeaturesReceive
	}

	if Args.kind == "transform" {
		convert?:  #FeaturesConvert
		enrich?:   #FeaturesEnrich
		filter?:   #FeaturesFilter
		parse?:    #FeaturesParse
		program?:  #FeaturesProgram
		reduce?:   #FeaturesReduce
		route?:    #FeaturesRoute
		sanitize?: #FeaturesSanitize
		shape?:    #FeaturesShape
	}

	if Args.kind == "sink" {
		// `buffer` describes how the component buffers data.
		buffer: close({
			enabled: bool | string
		})

		// `healtcheck` notes if a component offers a healthcheck on boot.
		healthcheck: close({
			enabled: bool
		})

		exposes?: #FeaturesExpose
		send?:    #FeaturesSend & {_args: Args}
	}

	descriptions: [Name=string]: string
}

#FeaturesCollect: {
	checkpoint: close({
		enabled: bool
	})

	from?: #Service
	tls?:  #FeaturesTLS & {_args: {mode: "connect"}}
}

#FeaturesConvert: {
}

#FeaturesEnrich: {
	from: close({
		name:     string
		url:      string
		versions: string | null
	})
}

#FeaturesExpose: {
	for: #Service
}

#FeaturesFilter: {
}

#FeaturesGenerate: {
}

#FeaturesMultiline: {
	enabled: bool
}

#FeaturesParse: {
	format: close({
		name:     string
		url:      string | null
		versions: string | null
	})
}

#FeaturesProgram: {
	runtime: #Runtime
}

#FeaturesReceive: {
	from?: #Service
	tls:   #FeaturesTLS & {_args: {mode: "accept"}}
}

#FeaturesReduce: {
}

#FeaturesRoute: {
}

#FeaturesSanitize: {
}

#FeaturesShape: {
}

#FeaturesSend: {
	_args: {
		egress_method: string
		kind:          string
	}
	let Args = _args

	if Args.egress_method == "batch" {
		// `batch` describes how the component batches data. This is only
		// relevant if a component has an `egress_method` of "batch".
		batch: close({
			enabled:      bool
			common:       bool
			max_bytes:    uint | null
			max_events:   uint | null
			timeout_secs: uint16
		})
	}

	// `compression` describes how the component compresses data.
	compression: {
		enabled: bool

		if enabled == true {
			default: #CompressionAlgorithm
			algorithms: [#CompressionAlgorithm, ...]
			levels: [#CompressionLevel, ...]
		}
	}

	// `encoding` describes how the component encodes data.
	encoding: {
		enabled: bool

		if enabled {
			codec: {
				enabled: bool

				if enabled {
					default: #EncodingCodec | null
					enum:    [#EncodingCodec, ...] | null
				}
			}
		}
	}

	// `request` describes how the component issues and manages external
	// requests.
	request: {
		enabled: bool

		if enabled {
			in_flight_limit:            uint8 | *5
			rate_limit_duration_secs:   uint8
			rate_limit_num:             uint16
			retry_initial_backoff_secs: uint8
			retry_max_duration_secs:    uint8
			timeout_secs:               uint8
		}
	}

	// `tls` describes if the component secures network communication
	// via TLS.
	tls: #FeaturesTLS & {_args: {mode: "connect"}}

	to?: #Service
}

#FeaturesTLS: {
	_args: {
		mode: "accept" | "connect"
	}
	let Args = _args
	enabled: bool

	if enabled {
		can_enable:             bool
		can_verify_certificate: bool
		if Args.mode == "connect" {
			can_verify_hostname: bool
		}
		enabled_default: bool
	}
}

#HowItWorks: [Name=string]: close({
	name:  Name
	title: string
	body:  string
	sub_sections?: [...{
		title: string
		body:  string
	}]
})

#Input: {
	logs:    bool
	metrics: #MetricInput | null
}

#LogEvent: {
	host?:      string | null
	message?:   string | null
	timestamp?: string | null
	#Any
}

#LogOutput: [Name=string]: close({
	description: string
	name:        Name
	fields:      #Schema
})

#MetricInput: {
	counter:      bool
	distribution: bool
	gauge:        bool
	histogram:    bool
	summary:      bool
	set:          bool
}

#MetricEvent: {
	kind: "incremental" | "absolute"
	name: string
	tags: [Name=string]: string
	timestamp?: string
	close({counter: #MetricEventCounter}) |
	close({distribution: #MetricEventDistribution}) |
	close({gauge: #MetricEventGauge}) |
	close({histogram: #MetricEventHistogram}) |
	close({set: #MetricEventSet}) |
	close({summary: #MetricEventSummary})
}

#MetricEventCounter: {
	value: float
}

#MetricEventDistribution: {
	values: [float, ...]
	sample_rates: [uint, ...]
	statistic: "histogram" | "summary"
}

#MetricEventGauge: {
	value: float
}

#MetricEventHistogram: {
	buckets: [float, ...]
	counts: [int, ...]
	count: int
	sum:   float
}

#MetricEventSet: {
	values: [string, ...]
}

#MetricEventSummary: {
	quantiles: [float, ...]
	values: [float, ...]
	count: int
	sum:   float
}

#MetricOutput: [Name=string]: close({
	description:    string
	relevant_when?: string
	tags:           #MetricTags
	name:           Name
	type:           #MetricType
})

#MetricTags: [Name=string]: close({
	description: string
	examples: [string, ...]
	required: bool
	name:     Name
})

#MetricType: "counter" | "distribution" | "gauge" | "histogram" | "summary"

#Object: {[_=string]: #Any}

#Output: {
	logs?:    #LogOutput
	metrics?: #MetricOutput
}

#Permissions: {
	unix: {
		group: string
	}
}

#Platforms: {
	"aarch64-unknown-linux-gnu":  bool
	"aarch64-unknown-linux-musl": bool
	"x86_64-apple-darwin":        bool
	"x86_64-pc-windows-msv":      bool
	"x86_64-unknown-linux-gnu":   bool
	"x86_64-unknown-linux-musl":  bool
}

#Protocol: "http" | "tcp" | "udp" | "unix"

#Releases: [Name=string]: {
	codename: string
	date:     string

	commits: [#Commit, ...]
	whats_next: #Any
}

#Runtime: {
	name:    string
	url:     string
	version: string | null
}

#Service: {
	name:     string
	thing:    string
	url:      string
	versions: string | null

	interface?: #Interface

	setup: [...string]
}

#Schema: [Name=string]: {
	// `category` allows you to group options into categories.
	//
	// For example, all of the `*_key` options might be grouped under the
	// "Context" category to make generated configuration examples easier to
	// read.
	category?: string

	if type.object != _|_ {
		category: strings.ToTitle(name)
	}

	// `desription` describes the option in a succinct fashion. Usually 1 to
	// 2 sentences.
	description: string

	// `groups` groups options into categories.
	//
	// For example, the `influxdb_logs` sink supports both v1 and v2 of Influxdb
	// and relevant options are placed in those groups.
	groups?: [...string]

	// `name` sets the name for this option. It is automatically set for you
	// via the key you use.
	name: Name

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
	warnings: [...string]

	if !required {
		// `common` specifes that the option is commonly used. It will bring the
		// option to the top of the documents, surfacing it from other
		// less common, options.
		common: bool
	}

	// `sort` sorts the option, otherwise options will be sorted alphabetically.
	sort?: int8

	// `types` sets the option's value type. External tagging is used since
	// each type has its own set of fields.
	type: #Type & {_args: "required": required}
}

#Support: {
	_args: kind: string

	// `platforms` describes which platforms this component is available on.
	//
	// For example, the `journald` source is only available on Linux
	// environments.
	platforms: #Platforms

	// `requirements` describes any external requirements that the component
	// needs to function properly.
	//
	// For example, the `journald` source requires the presence of the
	// `journalctl` binary.
	requirements: [...string] | null

	// `warnings` describes any warnings the user should know about the
	// component.
	//
	// For example, the `grok_parser` might offer a performance warning
	// since the `regex_parser` and other transforms are faster.
	warnings: [...string] | null

	// `notices` communicates useful information to the user that is neither
	// a requirement or a warning.
	//
	// For example, the `lua` transform offers a Lua version notice that
	// communicate which version of Lua is embedded.
	notices: [...string] | null
}

#Timestamp: =~"^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}.\\d{6}Z"

#Type: {
	_args: {
		arrays:   true
		required: bool
	}
	let Args = _args

	// `*` represents a wildcard type.
	//
	// For example, the `sinks.http.headers.*` option allows for arbitrary
	// key/value pairs.
	close({"array": #TypeArray & {_args: required: Args.required}}) |
	#TypePrimitive
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
	close({"*": close({})}) |
	close({"bool": #TypeBool & {_args: required: Args.required}}) |
	close({"float": #TypeFloat & {_args: required: Args.required}}) |
	close({"object": #TypeObject & {_args: required: Args.required}}) |
	close({"string": #TypeString & {_args: required: Args.required}}) |
	close({"timestamp": #TypeTimestamp & {_args: required: Args.required}}) |
	close({"uint": #TypeUint & {_args: required: Args.required}})
}

#TypeArray: {
	_args: required: bool
	let Args = _args

	if !Args.required {
		// `default` sets the default value.
		default: [...] | null
	}

	// Set `required` to `true` to force disable defaults. Defaults should
	// be specified on the array level and not the type level.
	items: type: #TypePrimitive & {_args: required: true}
}

#TypeBool: {
	_args: required: bool
	let Args = _args

	if !Args.required {
		// `default` sets the default value.
		default: bool | null
	}
}

#TypeFloat: {
	_args: required: bool
	let Args = _args

	if !Args.required {
		// `default` sets the default value.
		default: float | null
	}

	// `examples` clarify values through examples. This should be used
	// when examples cannot be derived from the `default` or `enum`
	// options.
	examples?: [...float]
}

#TypeObject: {
	// `examples` clarify values through examples. This should be used
	// when examples cannot be derived from the `default` or `enum`
	// options.
	examples: [#Object] | *[]

	// `options` represent the child options for this option.
	options: #Schema
}

#TypeString: {
	_args: required: bool
	let Args = _args

	if !Args.required {
		// `default` sets the default value.
		default: string | null
	}

	// `enum` restricts the value to a set of values.
	//
	//      enum: {
	//       json: "Encodes the data via application/json"
	//       text: "Encodes the data via text/plain"
	//      }
	enum?: #Enum

	if enum == _|_ {
		// `examples` demonstrates example values. This should be used when
		// examples cannot be derived from the `default` or `enum` options.
		examples: [...string] | *[
				for k, v in enum {
				k
			},
		]
	}

	// `templateable` means that the option supports dynamic templated
	// values.
	templateable?: bool
}

#TypeTimestamp: {
	_args: required: bool
	let Args = _args

	if !Args.required {
		// `default` sets the default value.
		default: #Timestamp | null
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
		default: uint | null
	}

	// `examples` clarify values through examples. This should be used
	// when examples cannot be derived from the `default` or `enum`
	// options.
	examples?: [...uint]

	// `unit` clarifies the value's unit. While this should be included
	// as the suffix in the name, this helps to explicitly clarify that.
	unit: #Unit | null
}

#Unit: "bytes" | "events" | "milliseconds" | "requests" | "seconds"

components: close({
	sources:    #Components
	transforms: #Components
	sinks:      #Components
})

data_model: close({
	schema: #Schema
})

releases: #Releases

remap: {
	errors: [Name=string]: {
		description: string
		name:        Name
	}

	functions: [Name=string]: {
		arguments: [
			...{
				required: bool

				if !required {
					name: string
				}

				type: "float" | "int" | "string"
			},
		]
		category:    "coerce" | "parse"
		description: string
		examples: [
			{
				title:  string
				input:  #Fields
				source: string
				output: #Fields
			},
			...,
		]
		name: Name
	}
}
