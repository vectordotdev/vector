package metadata

components: transforms: wasm: {
	title: "WASM"

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "stream"
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
			"aarch64-unknown-linux-gnu":  false
			"aarch64-unknown-linux-musl": false
			"x86_64-apple-darwin":        false
			"x86_64-pc-windows-msv":      false
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  false
		}

		requirements: [
			#"""
				Vector must be build with the `wasm` feature. *This is not enabled by default. Review [Building Vector][urls.contributing]*.
				"""#,
		]
		warnings: []
		notices: []
	}

	configuration: {
		artifact_cache: {
			description: "The directory where Vector should store the artifact it builds of this WASM module. Typically, all WASM modules share this."
			required:    true
			warnings: []
			type: string: examples: [
				"/etc/vector/artifacts",
				"/var/lib/vector/artifacts",
				"C:\\vector\\artifacts",
			]
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
			type: string: examples: [
				"./modules/example.wasm",
				"/example.wat",
				"example.wasm",
			]
		}
	}

	input: {
		logs:    true
		metrics: null
	}
}
