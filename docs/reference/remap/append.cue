package metadata

remap: functions: append: {
	arguments: [
		{
			name:        "value"
			description: "The array"
			required:    true
			type: ["array"]
		},
		{
			name:        "item"
			description: "The items to append"
			required:    true
			type: ["array"]
		},
	]
	return: ["array"]
	category: "Array"
	description: """
		Adds the specified array to the end of an array and returns the resulting array. The items
		can be of any VRL type and are added even if items with the same value are already present
		in the array.

		The `append` function does *not* change the array in place. In this example, the `append`
		function would return an array with `apple`, `orange`, and `banana`, but the value of
		`fruits` would be unchanged:

		```js
		.fruits = ["apple", "orange"]
		append(.fruits, ["banana"])
		```

		In order to change the value of `fruits`, you would need to store the resulting array in
		the field:

		```js
		.fruits = append(.fruits, ["banana"])
		```
		"""
	examples: [
		{
			title: "Mixed array"
			input: {
				kitchen_sink: [72.5, false, [1, 2, 3]]
				items: ["booper", "bopper"]
			}
			source: """
				.kitchen_sink = append(.kitchen_sink, .items)
				"""
			output: {
				kitchen_sink: [72.5, false, [1, 2, 3], "booper", "bopper"]
			}
		},
	]
}
