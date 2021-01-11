package metadata

remap: functions: push: {
	arguments: [
		{
			name:        "value"
			description: "The array"
			required:    true
			type: ["array"]
		},
		{
			name:        "item"
			description: "The item to push"
			required:    true
			type: ["any"]
		},
	]
	return: ["array"]
	category: "Array"
	description: """
		Adds the specified item to the end of an array and returns the resulting array. The item
		can be of any VRL type and is added even if an item with the same value is already present
		in the array.

		The `push` function does *not* change the array in place. In this example, the `push`
		function would return an array with `apple`, `orange`, and `banana`, but the value of
		`fruits` would be unchanged:

		```js
		.fruits = ["apple", "orange"]
		push(.fruits, "banana")
		.fruits
		["apple", "orange"]
		```

		In order to change the value of `fruits`, you would need to store the resulting array in
		the field:

		```js
		.fruits = push(.fruits, "banana")
		```
		"""
	examples: [
		{
			title: "Mixed array"
			input: {
				kitchen_sink: [72.5, false, [1, 2, 3]]
				item: "booper"
			}
			source: """
				.kitchen_sink = push(.kitchen_sink, .item)
				"""
			output: {
				kitchen_sink: [72.5, false, [1, 2, 3], "booper"]
			}
		},
	]
}
