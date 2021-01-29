remap: features: progressive_type_safety: {
	title: "Progressive type safety"
	description: """
		VRL implements _progressive_ type safety, erroring at compilation-time if a type mismatch is detected. This
		largely contributes to VRL's safety principle, ensuring scripts work as expected post-compilation.
		"""

	principles: {
		performance: false
		safety:      false
	}

	characteristics: {
		errors: {
			title: "Errors"
			description: """
				If VRL detects a type mismatch, it will produce a user-friendly compile-time error. For example, given
				this VRL script:

				```vrl
				foo, err = 5
				```

				VRL will produce the following compile-time error:

				```
				error: unneeded error assignment
				  ┌─ :1:1
				  │
				1 │ foo, err = 5;
				  │ ^^^^^^^^   - because this expression cannot fail
				  │ │
				  │ this error assignment is unneeded
				  │
				  = hint: assign to "foo", without assigning to "err"
				  = see language documentation at: https://vector.dev
				```
				"""
		}
		progressive: {
			title: "Progressivenes"
			description: """
				VRL's type safety is _progressive_, meaning it will implement type safety for any value for which it
				knows the type. Because observability data can be quite unpredictable, it's not always known which
				type a field might be, hence the _progressive_ nature of VRL's type safety. As VRL scripts are
				evaluated, type information is built up and used at compile-time to enforce type safety. Let's look
				at an example:

				```vrl
				.foo # any
				.foo = downcase!(.foo) # string
				.foo = upcase(.foo) # string
				```

				Breaking down the above:

				1. The `.foo` field starts off as an `any` type (AKA unknown).
				2. The call to the `downcase!` function requires error handling (`!`) since VRL cannot guarantee that
				   `.foo` is a string (the only type supported by `downcase`).
				3. Afterwards, assuming the `downcase` invocation is successful, VRL knows that `.foo` is a string,
				   since `downcase` can only return strings.
				4. Finally, the call to `upcase` does not require error handling (`!`) since VRL knows that `.foo` is a
				   string, making the `upcase` invocation infallible.

				To avoid error handling for argument errors, you can specify the types of your fields at the top
				of your VRL script:

				```vrl
				.foo = to_string!(.foo) # string
				.foo = downcase(.foo) # string
				```

				This is generally good practice, and it provides the ability to opt-into type safety as you see fit,
				VRL scripts are written once and evaluated many times, therefore the tradeoff for type safety will
				ensure reliable production execution.
				"""
		}
	}
}
