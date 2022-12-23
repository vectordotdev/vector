package metadata

base: components: transforms: configuration: inputs: {
	description: """
		A list of upstream [source][sources] or [transform][transforms] IDs.

		Wildcards (`*`) are supported.

		See [configuration][configuration] for more info.

		[sources]: https://vector.dev/docs/reference/configuration/sources/
		[transforms]: https://vector.dev/docs/reference/configuration/transforms/
		[configuration]: https://vector.dev/docs/reference/configuration/
		"""
	required: true
	type: array: items: type: string: examples: ["my-source-or-transform-id", "prefix-*"]
}
