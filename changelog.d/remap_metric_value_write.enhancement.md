The `remap` transform can now set the value of a counter or gauge by assigning an integer or a float to
`.counter.value` or `.gauge.value`, for example, `.gauge.value = 100`. The path must match the metric's
current type, and the value is stored as a float. Other metric value fields remain read-only.

authors: JinchengCao
