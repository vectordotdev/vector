#!/bin/bash
for i in $(seq 1 3); do
    BIND_ROOT="/bind${i}"
    BIND_CONTROL_PORT="900${i}"

    # Set up folders and files needed for the this instance.
    mkdir -p "${BIND_ROOT}/etc/bind/socket" "${BIND_ROOT}/usr/share/dns" "${BIND_ROOT}/var/lib/bind"
    cp -r /etc/bind/* "${BIND_ROOT}/etc/bind/"
    cp /usr/share/dns/root.hints "${BIND_ROOT}/usr/share/dns/"
    cp /var/lib/bind/db.example.com "${BIND_ROOT}/var/lib/bind/"

    # Copy the configuration template and update it for this specific instance.
    mv "${BIND_ROOT}/etc/bind/named.conf.options.template" "${BIND_ROOT}/etc/bind/named.conf.options"
    sed -i "s/dnstap.sock#/dnstap.sock${i}/" "${BIND_ROOT}/etc/bind/named.conf.options"
    sed -i "s/port #/port ${BIND_CONTROL_PORT}/" "${BIND_ROOT}/etc/bind/named.conf.options"
done
