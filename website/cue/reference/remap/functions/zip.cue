package metadata

remap: functions: zip: {
	category: "Array"
	description: """
		Iterate over several arrays in parallel, producing a new array containing arrays of items from each source.
		The resulting array will be as long as the shortest input array, with all the remaining elements dropped.
		This function is modeled from the `zip` function [in Python](https://docs.python.org/3/library/functions.html#zip),
		but similar methods can be found in [Ruby](https://docs.ruby-lang.org/en/master/Array.html#method-i-zip)
		and [Rust](https://doc.rust-lang.org/stable/std/iter/trait.Iterator.html#method.zip).

		If a single parameter is given, it must contain an array of all the input arrays.
		"""

	arguments: [
		{
			name:        "array_0"
			description: "The first array of elements, or the array of input arrays if no other parameter is present."
			required:    true
			type: ["array"]
		},
		{
			name:        "array_1"
			description: "The second array of elements. If not present, the first parameter contains all the arrays."
			required:    false
			type: ["array"]
		},
	]
	internal_failure_reasons: [
		"`array_0` and `array_1` must be arrays.",
	]
	return: {
		types: ["array"]
		rules: [
			"`zip` is considered fallible if any of the parameters is not an array, or if only the first parameter is present and it is not an array of arrays.",
		]
	}

	examples: [
		{
			title: "Merge two arrays"
			source: #"""
				zip([1, 2, 3], [4, 5, 6, 7])
				"""#
			return: [[1, 4], [2, 5], [3, 6]]
		},
		{
			title: "Merge three arrays"
			source: #"""
				zip([[1, 2], [3, 4], [5, 6]])
				"""#
			return: [[1, 3, 5], [2, 4, 6]]
		},
	]
}
