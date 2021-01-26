package metadata

components: transforms: wasm: {
	title: "WASM"

	description: """
		Process events using the [WASM](\(urls.wasm)) virtual machine, allowing
		you to process Vector events with Typescript, Ruby, Java, and [more](\(urls.wasm_languages)).
		"""

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "stream"
		stateful:      true
	}

	features: {
		program: {
			runtime: {
				name:    "WASM"
				url:     urls.wasm
				version: null
			}
		}
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      false
			"aarch64-unknown-linux-musl":     false
			"armv7-unknown-linux-gnueabihf":  false
			"armv7-unknown-linux-musleabihf": false
			"x86_64-apple-darwin":            false
			"x86_64-pc-windows-msv":          false
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      false
		}

		requirements: [
			#"""
				Vector must be built with the `wasm` feature. *This is not enabled by default.
				Review [Building Vector][urls.contributing]*.
				"""#,
		]
		warnings: []
		notices: [
			"""
			Please consider the [`remap` transform](\(urls.vector_remap_transform)) before using this tranform. The
			[Vector Remap Language](\(urls.vrl_reference)) is designed for safe, performant, and easy data mapping. It
			is intended to cover the vast majority of data mapping use cases leaving WASM for very advanced and
			edge-case situations.
			""",
		]
	}

	configuration: {
		artifact_cache: {
			description: "The directory where Vector should store the artifact it builds of this WASM module. Typically, all WASM modules share this."
			required:    true
			warnings: []
			type: string: {
				examples: [
					"/etc/vector/artifacts",
					"/var/lib/vector/artifacts",
					"C:\\vector\\artifacts",
				]
				syntax: "file_system_path"
			}
		}
		heap_max_size: {
			common:      false
			description: "The maximum size of the heap of this module, in bytes. (This includes the module itself, default is 10 MB.)"
			required:    false
			warnings: []
			type: uint: {
				default: 10485760
				unit:    "bytes"
			}
		}
		module: {
			description: "The file path of the `.wasm` or `.wat` module."
			required:    true
			warnings: []
			type: string: {
				examples: [
					"./modules/example.wasm",
					"/example.wat",
					"example.wasm",
				]
				syntax: "file_system_path"
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}
}
