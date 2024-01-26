#Event: {log: #Log} | {metric: #Metric} | {trace: #Trace}

#Log: {...}

#Trace: {...}

#Metric: {
	name:       string
	namespace?: string
	tags?: {[string]: #TagValueSet}
	timestamp?:   #Timestamp
	interval_ms?: int
	kind:         "incremental" | "absolute"
	{counter: value: number} |
	{gauge: value: number} |
	{set: values: [...string]} |
	{distribution: {
		samples: [...{value: number, rate: int}]
		statistic: "histogram" | "summary"
	}} |
	{aggregated_histogram: {
		buckets: [...{upper_limit: number, count: int}]
		count: int
		sum:   number
	}} |
	{aggregated_summary: {
		quantiles: [...{quantile: number, value: number}]
		count: int
		sum:   number
	}} |
	{sketch:
		sketch: AgentDDSketch: {
			bins: {
				k: [...int]
				n: [...int]
			}
			count: int
			min:   number
			max:   number
			sum:   number
			avg:   number
		}
}
}

#TagValueSet: {#TagValue | [...#TagValue]}

#TagValue: {string | null}

#Timestamp: =~"^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}(.\\d+)?Z"
