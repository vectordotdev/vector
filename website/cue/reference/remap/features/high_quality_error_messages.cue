remap: features: high_quality_error_messages: {
	title: "High-quality error messages"
	description: """
		VRL strives to provide high-quality, helpful error messages, streamlining the development and iteration workflow
		around VRL programs.

		This VRL program, for example...

		```coffee
		parse_json!(1)
		```

		...would result in this error:

		```rust
		error[E110]: invalid argument type
		  ┌─ :2:13
		  │
		2 │ parse_json!(1)
		  │             ^
		  │             │
		  │             this expression resolves to the exact type integer
		  │             but the parameter "value" expects the exact type string
		  │
		  = try: ensuring an appropriate type at runtime
		  =
		  =     1 = string!(1)
		  =     parse_json!(1)
		  =
		  = try: coercing to an appropriate type and specifying a default value as a fallback in case coercion fails
		  =
		  =     1 = to_string(1) ?? "default"
		  =     parse_json!(1)
		  =
		  = see documentation about error handling at https://errors.vrl.dev/#handling
		  = learn more about error code 110 at https://errors.vrl.dev/110
		  = see language documentation at https://vrl.dev
		  = try your code in the VRL REPL, learn more at https://vrl.dev/examples
		```
		"""

	principles: {
		performance: false
		safety:      false
	}
}
