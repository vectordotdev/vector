package metadata

remap: functions: map_keys: {
	category: "Enumerate"
	description: #"""
		Map the keys within an object.

		If `recursive` is enabled, the function iterates into nested
		objects, using the following rules:

		1. Iteration starts at the root.
		2. For every nested object type:
		   - First return the key of the object type itself.
		   - Then recurse into the object, and loop back to item (1)
		     in this list.
		   - Any mutation done on a nested object *before* recursing into
		     it, are preserved.
		3. For every nested array type:
		   - First return the key of the array type itself.
		   - Then find all objects within the array, and apply item (2)
		     to each individual object.

		The above rules mean that `map_keys` with
		`recursive` enabled finds *all* keys in the target,
		regardless of whether nested objects are nested inside arrays.

		The function uses the function closure syntax to allow reading
		the key for each item in the object.

		The same scoping rules apply to closure blocks as they do for
		regular blocks. This means that any variable defined in parent scopes
		is accessible, and mutations to those variables are preserved,
		but any new variables instantiated in the closure block are
		unavailable outside of the block.

		See the examples below to learn about the closure syntax.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The object to iterate."
			required:    true
			type: ["object"]
		},
		{
			name:        "recursive"
			description: "Whether to recursively iterate the collection."
			required:    false
			default:     false
			type: ["boolean"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["object"]
	}
	examples: [
		{
			title: "Upcase keys"
			input: log: {
				foo: "foo"
				bar: "bar"
			}
			source: #"""
				map_keys(.) -> |key| { upcase(key) }
				"""#
			return: {"FOO": "foo", "BAR": "bar"}
		},
		{
			title: "De-dot keys"
			input: log: {
				labels: {
					"app.kubernetes.io/name": "mysql"
				}
			}
			source: #"""
				map_keys(., recursive: true) -> |key| { replace(key, ".", "_") }
				"""#
			return: {
				labels: {
					"app_kubernetes_io/name": "mysql"
				}
			}
		},
	]
}
