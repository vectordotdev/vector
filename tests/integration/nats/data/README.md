# Generating configurations and credentials

## Before You Begin

The TLS fixtures use a dedicated test CA and the public test keys in this directory. Use OpenSSL 3 to regenerate the
certificates; do not install this CA in your system trust store.

Additionally, all commands which generate output files -- such as in the TLS and JWT sections -- need to either be run
from the correct test data directory -- `tests/integration/nats/data` -- or need to have those files moved to that directory.

## Generating the "NKey"-based configuration/credentials

The NATS guide for using
[NKeys](https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro/nkey_auth) is the best
resource to use, and includes all the required commands configuration snippets necessary for the configuration.

## Generating the TLS-based configuration/credentials

Run the following from `tests/integration/nats/data`. It reuses the checked-in server and client test keys and creates a
temporary CA key that is deleted afterward. The certificates remain valid for 20 years to avoid frequent CI interruptions.
The server names cover local runs and the Compose services; `email@localhost` is required by the mutual TLS user mapping.

```shell
fixture_tmp=$(mktemp -d)
trap 'rm -rf "$fixture_tmp"' EXIT
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "$fixture_tmp/rootCA.key" -out rootCA.pem -days 7301 \
  -subj '/CN=Vector NATS integration test CA' \
  -addext 'basicConstraints=critical,CA:TRUE,pathlen:0' \
  -addext 'keyUsage=critical,keyCertSign,cRLSign' \
  -addext 'subjectKeyIdentifier=hash'

cat > "$fixture_tmp/extensions.cnf" <<'EOF'
[server]
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
subjectAltName=DNS:localhost,DNS:nats-tls,DNS:nats-tls-client-cert,DNS:nats-jwt,IP:::1
subjectKeyIdentifier=hash
authorityKeyIdentifier=keyid,issuer

[client]
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=clientAuth,serverAuth,emailProtection
subjectAltName=DNS:localhost,DNS:nats-tls,DNS:nats-tls-client-cert,DNS:nats-jwt,IP:::1,email:email@localhost
subjectKeyIdentifier=hash
authorityKeyIdentifier=keyid,issuer
EOF

for kind in server client; do
  openssl req -new -key "nats-$kind.key" -out "$fixture_tmp/$kind.csr" \
    -subj "/CN=Vector NATS integration test $kind"
  openssl x509 -req -in "$fixture_tmp/$kind.csr" -out "nats-$kind.pem" \
    -CA rootCA.pem -CAkey "$fixture_tmp/rootCA.key" \
    -CAcreateserial -CAserial "$fixture_tmp/rootCA.srl" -days 7300 \
    -extfile "$fixture_tmp/extensions.cnf" -extensions "$kind"
done
```

Verify the chain and intended certificate usages:

```shell
openssl verify -CAfile rootCA.pem -purpose sslserver nats-server.pem
openssl verify -CAfile rootCA.pem -purpose sslclient nats-client.pem
```

Keep `rootCA.pem`, `nats-server.pem`, and `nats-client.pem` together when updating these fixtures.

## Generating the JWT-based configuration credentials

You'll first need to follow the steps for [JWT authentication in
NATS](https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro/jwt/mem_resolver), which
requires first generating the necessary TLS certificates, so don't skip the above section.

Next, we'll need to move the credentials that the above guide had you generate.

```shell
# Now move these credentials to the right location, since they don't just drop into the current directory.
$ mv ~/.nkeys/creds/memory/A/TA.creds tests/integration/nats/data/nats.creds

# After that, we make a copy that will act as the "bad" credentials.
#
# You'll need to open the "bad" version and change one of the characters in the seed value in order to actually make them "bad". :)
$ cp tests/integration/nats/data/nats.creds tests/integration/nats/data/nats-bad.creds
```
