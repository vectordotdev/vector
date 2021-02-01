remap: features: vector_native: {
	title: "Vector native"
	description: """
		VRL is native to Vector and purpose-built for use within Vector. VRL does not pay a penalty to receive and
		return Vector events. This makes VRL significantly faster than alternative runtimes, like Lua, where data must
		be serialized as it's passed to and from the runtime.
		"""

	principles: {
		performance: true
		safety:      false
	}
}
