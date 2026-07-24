Fixed the `vector_security_confinement_disabled` internal metric disappearing after the metric idle timeout (300 seconds by default) while a sink was still running with `dangerously_allow_unconfined_template_resolution` enabled. The gauge is now owned by the topology and held for the lifetime of each sink, and refreshed on configuration reload, so alerts watching this metric no longer silently stop firing.

authors: thomasqueirozb
