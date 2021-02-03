package metadata

remap: errors: "102": {
	title:       "Non-boolean if expression predicate"
	description: """
		An [if expression](\(urls.vrl_expressions)#\(remap.literals.regular_expression.anchor)) predicate does not
		evaluate to a boolean.
		"""
	rationale:   """
		VRL does not implement "truthy" values (non-boolean values that resolve to a boolean, such as `1`) since these
		are common foot-guns that can result in unexpected behavior when used in if expressions. This decision
		contributes to VRL's [safety principle](\(urls.vrl_safety)), ensuring that VRL programs are reliable once
		deployed.
		"""
	resolution:  """
		Adjust your if expression predicate to resolve to a boolean. Helpful functions to solve this include
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
			raises: compiletime: #"""
				error: \#(title)
				  ┌─ :1:1
				  │
				1 │ 	if .message {
				  │        ^^^^^^^^
				  │        │
				  │        if expression predicates must resolve to a strict boolean
				  │
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
