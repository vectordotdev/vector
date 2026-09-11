package metadata

_schemaDefinitions: "vector::http::ParameterValue": {
	object: options: {
		type: {
			description: "The parameter type, indicating how the `value` should be treated."
			required:    false
			type: string: {
				default: "string"
				enum: {
					string: "The parameter value is a plain string."
					vrl:    "The parameter value is a VRL expression that is evaluated before each request."
				}
			}
		}
		value: {
			description: "The raw value of the parameter."
			required:    true
			type: string: {}
		}
	}
	string: {}
}
