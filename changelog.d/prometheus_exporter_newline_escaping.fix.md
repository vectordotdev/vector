Ensure the `prometheus_exporter` sink emits valid text exposition by escaping newlines in label
values and rejecting metrics whose metric names, namespaces, or label names contain carriage returns
or newlines.

authors: pront
