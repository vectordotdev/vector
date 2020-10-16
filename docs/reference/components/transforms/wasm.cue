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
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
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
