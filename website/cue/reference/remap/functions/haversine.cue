package metadata

remap: functions: haversine: {
	category:    "Map"
	description: """
		Calculates [haversine](\(urls.haversine)) distance and bearing between two points.
		Results are available in kilometers or miles.
		"""

	arguments: [
		{
			name:        "lat1"
			description: "Latitude of the first point."
			required:    true
			type: ["float"]
		},
		{
			name:        "lon1"
			description: "Longitude of the first point."
			required:    true
			type: ["float"]
		},
		{
			name:        "lat2"
			description: "Latitude of the second point."
			required:    true
			type: ["float"]
		},
		{
			name:        "lon2"
			description: "Longitude of the second point."
			required:    true
			type: ["float"]
		},
		{
			name: "measurement"
			description: "Measurement system to use for resulting distance."
			required: false
			type: ["string"]
			default: "kilometers"
			enum: {
				kilometers: "Use kilometers for the resulting distance."
				miles:      "Use miles for the resulting distance."
			}
		},
	]
	internal_failure_reasons: []
	return: types: ["object"]

	examples: [
		{
			title: "Haversine in kilometers"
			source: #"""
				haversine(0, 0, 10, 10)
				"""#
			return: {
				distance: 1568.5227233,
				bearing: 44.561
			}
		},
		{
			title: "Haversine in miles"
			source: #"""
				haversine(0, 0, 10, 10, "miles")
				"""#
			return: {
				distance: 974.6348468,
				bearing: 44.561
			}
		},
	]
}
