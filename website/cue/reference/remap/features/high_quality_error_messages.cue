remap: features: high_quality_error_messages: {
	title: "High-quality error messages"
	description: """
		VRL strives to provide high-quality, helpful error messages, streamlining the development and iteration workflow
		around VRL programs.

		This VRL program, for example...

		```ruby
		.foo, err = upcase(.foo)
		```

		...would result in this error:

		```rust
		error: program aborted
		  ┌─ :2:1
		  │
		2 │ parse_json!(1)
		  │ ^^^^^^^^^^^^^^
		  │ │
		  │ function call error
		  │ unable to parse json: key must be a string at line 1 column 3
		  │
		  = see function documentation at: https://master.vector.dev/docs/reference/vrl/functions/#parse_json
		  = see language documentation at: https://vector.dev/docs/reference/vrl/
		```
		"""

	principles: {
		performance: false
		safety:      false
	}
}
