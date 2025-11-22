The `splunk_hec` sink now flattens nested objects and arrays.

Previously the Splunk HEC api would reject events that contained them.

Notable changes:

- Objects will be flattened
- Arrays that contain objects or arrays will be flattened
- Flat arrays are permitted
- Numbers will be stringified

authors: matt-simons
