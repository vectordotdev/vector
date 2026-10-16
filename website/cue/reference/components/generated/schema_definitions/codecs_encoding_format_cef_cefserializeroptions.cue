package metadata

_schemaDefinitions: "codecs::encoding::format::cef::CefSerializerOptions": object: options: {
	device_event_class_id: {
		description: """
			Unique identifier for each event type. Identifies the type of event reported.
			The value length must be less than or equal to 1023.
			"""
		required: true
		type: string: {}
	}
	device_product: {
		description: """
			Identifies the product of a vendor.
			The part of a unique device identifier. No two products can use the same combination of device vendor and device product.
			The value length must be less than or equal to 63.
			"""
		required: true
		type: string: {}
	}
	device_vendor: {
		description: """
			Identifies the vendor of the product.
			The part of a unique device identifier. No two products can use the same combination of device vendor and device product.
			The value length must be less than or equal to 63.
			"""
		required: true
		type: string: {}
	}
	device_version: {
		description: """
			Identifies the version of the problem. The combination of the device product, vendor, and this value make up the unique id of the device that sends messages.
			The value length must be less than or equal to 31.
			"""
		required: true
		type: string: {}
	}
	extensions: {
		description: """
			The collection of key-value pairs. Keys are the keys of the extensions, and values are paths that point to the extension values of a log event.
			The event can have any number of key-value pairs in any order.
			"""
		required: true
		type: object: options: "*": {
			description: "This is a path that points to the extension value of a log event."
			required:    true
			type: string: {}
		}
	}
	name: {
		description: """
			This is a path that points to the human-readable description of a log event.
			The value length must be less than or equal to 512.
			Equals "cef.name" by default.
			"""
		required: true
		type: string: {}
	}
	severity: {
		description: """
			This is a path that points to the field of a log event that reflects importance of the event.

			It must point to a number from 0 to 10.
			0 = lowest_importance, 10 = highest_importance.
			Set to "cef.severity" by default.
			"""
		required: true
		type: string: {}
	}
	version: {
		description: """
			CEF Version. Can be either 0 or 1.
			Set to "0" by default.
			"""
		required: true
		type: string: enum: {
			V0: "CEF specification version 0.1."
			V1: "CEF specification version 1.x."
		}
	}
}
