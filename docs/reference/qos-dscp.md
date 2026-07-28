# DSCP (IP_TOS) QoS Marking

Vector can optionally set the IP_TOS (DSCP) byte on sockets created by sources and sinks.

Configure using DSCP names or numeric TOS values:

```toml
[sources.my_tcp]
# ... other settings
ip_tos = "EF"      # or "CS5", "AF21", etc.
# ip_tos = 184     # numeric TOS byte
```

Supported names: CS0..CS7, AF11..AF43, EF. Names map to DSCP codes; the TOS byte is DSCP<<2 (ECN bits 0).
On unsupported platforms the setting is ignored with a warning.
