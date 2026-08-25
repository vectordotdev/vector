Fix a panic in Vector when a sink endpoint or URI contains a non-numeric port (e.g. `http://localhost:notaport`). Malformed URIs now produce a validation error instead of crashing Vector.

authors: thomasqueirozb
