#!/bin/bash

mkdir -p /ssl

cp /certs/postgres.key /ssl/postgres.key
chown postgres:postgres /ssl/postgres.key
chmod 600 /ssl/postgres.key

cp /certs/postgres.crt /ssl/postgres.crt

/docker-entrypoint.sh postgres \
    -c ssl=on \
    -c ssl_key_file=/ssl/postgres.key \
    -c ssl_cert_file=/ssl/postgres.crt \
    -c ssl_ca_file=/ssl/postgres.crt

