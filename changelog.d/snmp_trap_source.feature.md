A new `snmp_trap` source has been added to receive SNMP v1 and v2c trap messages over UDP (issue #4567)


The source listens for SNMP traps on a configurable UDP port (typically port 162) and converts them into log events. Each trap is parsed and its fields are extracted into structured log data, including community string, version, trap type, enterprise OID, and variable bindings.

authors: bachgarash
