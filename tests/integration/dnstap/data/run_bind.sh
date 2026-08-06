#!/bin/bash
for i in $(seq 1 3); do
    BIND_ROOT="/bind${i}"
    BIND_QUERY_PORT="800${i}"

	# Clean any leftover DNSTAP socket.
	rm -f "${BIND_ROOT}/etc/bind/socket/dnstap.sock${i}"

    # Bring up the BIND instance, which will spawn itself as a background daemon.
    /usr/sbin/named -p "${BIND_QUERY_PORT}" -t "${BIND_ROOT}"
done

# We need something to keep the container running, so we just... sleep forever.
# The BIND instances are just running in the background.
sleep infinity
