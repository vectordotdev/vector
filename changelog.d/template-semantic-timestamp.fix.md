Templates using strftime format specifiers now correctly use the semantic timestamp rather than
always looking for the `log_schema` timestamp. This is required when `log_namespacing` is enabled.
