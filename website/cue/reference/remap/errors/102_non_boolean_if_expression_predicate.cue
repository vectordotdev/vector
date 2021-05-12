package metadata

remap: errors: "102": {
	title:       "Non-boolean if expression predicate"
	description: """
		An [if expression](\(urls.vrl_expressions)#regular-expression) predicate doesn't
		evaluate to a Boolean.
		"""
	rationale:   """
		VRL doesn't implement "truthy" values (non-Boolean values that resolve to a Boolean, such as `1`) since these
		are common foot-guns that can result in unexpected behavior when used in if expressions. This provides important
		[safety guarantees](\(urls.vrl_safety)) in VRL and ensures that VRL programs are reliable once deployed.
		"""
	resolution:  """
		Adjust your if expression predicate to resolve to a Boolean. Helpful functions to solve this include
		[`exists`](\(urls.vrl_functions)#\(remap.functions.exists.anchor)) and
		[`is_nullish`](\(urls.vrl_functions)#\(remap.functions.is_nullish.anchor)).
		"""

	examples: [
		{
			"title": "\(title) (strings)"
			input: log: message: "key=value"
			source: #"""
				if .message {
					. |= parse_key_value!(.message)
				}
				"""#
			diff: #"""
				-if .message {
				+if exists(.message) {
				 	. |= parse_key_value!(.message)
				 }
				"""#
		},
	]
}
