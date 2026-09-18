Vector now rejects configurations that use an unavailable system local time zone instead of silently interpreting naive timestamps as UTC. If affected, set the global `timezone` explicitly or prepare the runtime environment with valid system time zone data.

authors: pront
