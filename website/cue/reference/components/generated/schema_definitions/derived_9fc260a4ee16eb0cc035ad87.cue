package metadata

_schemaDefinitions: "derived::9fc260a4ee16eb0cc035ad87": object: {
	examples: [
		{
			"X-Powered-By": "Vector"
		}]
	options: "*": {
		description: "Configuration of the query parameter value for HTTP requests."
		required:    true
		type:        _schemaDefinitions["vector::http::ParameterValue"]
	}
}
