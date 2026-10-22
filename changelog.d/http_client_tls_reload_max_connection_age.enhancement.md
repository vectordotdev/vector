The native HTTP client used by the Datadog sinks can now reload its TLS settings at runtime, for example after certificate rotation, and can optionally re-establish connections once they reach a maximum age. Both are opt-in for code that builds the client; Vector's default behavior is unchanged.

authors: taylorchandleryoung
