The `datadog_logs` source now does not require a semantic meaning input definition for `message` and `timestamp` fields.

While the Datadog logs intake does handle these fields if they are present, they aren't required.

If the are present (such as in the default, Legacy namespace), the behavior is unchanged.

The only impact is that configurations which enable the Log Namespace feature, no longer need to manually set the semantic meaning for these two fields through a remap transform if the source did not already set them.
