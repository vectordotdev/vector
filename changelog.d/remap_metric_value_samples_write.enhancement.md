The `remap` transform can now replace the samples of a distribution by assigning an array to
`.distribution.samples`, for example, `.distribution.samples = [{"value": 2.5, "rate": 1}]`. Each sample
needs exactly a `value` and a `rate` of at least 1, and the statistic is kept. Metric value fields other
than `.counter.value`, `.gauge.value`, and `.distribution.samples` remain read-only.

authors: JinchengCao
