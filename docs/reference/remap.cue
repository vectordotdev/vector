package metadata

#Remap: {
	#Characteristic: {
		title:       string
		description: string
	}

	#Example: {
		title:   string
		input?:  #Event
		source:  string
		raises?: string

		if raises == _|_ {
			return?: _
			output?: #Event
		}
	}

	#Type: "any" | "array" | "boolean" | "float" | "integer" | "map" | "null" | "path" | "string" | "regex" | "timestamp"

	concepts:    _
	description: string
	expressions: _
	features:    _
	functions:   _
	literals:    _
}

remap: #Remap & {
	description: #"""
		**Vector Remap Language** (VRL) is an [expression-oriented](\(urls.expression_oriented_language)) language
		specifically designed for transforming observability data (logs and metrics). It features a simple
		[syntax]((\urls.vrl_spec)) and a rich set of built-in [functions](\(urls.vrl_functions)) tailored specifically
		to observability use cases.

		### Principles

		VRL is built on the following two principles:

		1. **Performance** — VRL is implemented in the very fast and efficient [Rust](\(urls.rust)) language and
		   VRL scripts are compiled into Rust code when Vector is started. This means that you can use VRL to
		   transform observability data with a minimal per-event performance penalty vis-à-vis pure Rust. In addition,
		   ergonomic features such as compile-time correctness checks and the lack of language constructs like
		   loops make it difficult to write scripts that are slow or buggy or require optimization.
		2. **Safety** - VRL is a safe language in several senses: VRL scripts have access only to the event data
		   that they handle and not, for example, to the Internet or the host; VRL provides the same strong memory
		   safety guarantees as Rust; and, as mentioned above, compile-time correctness checks prevent VRL
		   scripts from behaving in unexpected or sub-optimal ways. These factors distinguish VRL from other
		   available event data transformation languages and runtimes.

		For a more in-depth picture, see the [announcement blog post](\(urls.vrl_announcement)) for more details.
		"""#
}
