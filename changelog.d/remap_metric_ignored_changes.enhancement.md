The `remap` transform now logs a rate-limited warning when a VRL program makes a change that a metric event can't accept, such as `.gauge.value = 100`, `del(.type)`, or a tag value that isn't a string or `null`. Previously these changes were ignored without any message. The event is still processed as before.

authors: JinchengCao
