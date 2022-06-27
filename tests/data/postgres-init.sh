#!/bin/bash

mkdir -p /ssl

cp /certs/intermediate_server/private/postgres.key.pem /ssl/postgres.key
chown postgres:postgres /ssl/postgres.key
chmod 600 /ssl/postgres.key

cp /certs/intermediate_server/certs/postgres-chain.cert.pem /ssl/postgres.crt

/docker-entrypoint.sh postgres \
    -c ssl=on \
    -c ssl_key_file=/ssl/postgres.key \
    -c ssl_cert_file=/ssl/postgres.crt \
    -c ssl_ca_file=/ssl/postgres.crt

