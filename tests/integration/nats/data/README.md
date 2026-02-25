# Generating configurations and credentials

## Before You Begin

The [`mkcert`](https://github.com/FiloSottile/mkcert) utility was used to generate the root CA, client, and server
certificates/keys.

Additionally, all commands which generate output files -- such as in the TLS and JWT sections -- need to either be run
from the correct test data directory -- `tests/data/nats` -- or need to have those files moved to that directory.

## Generating the "NKey"-based configuration/credentials

The NATS guide for using
[NKeys](https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro/nkey_auth) is the best
resource to use, and includes all the required commands configuration snippets necessary for the configuration.

## Generating the TLS-based configuration/credentials

First, we'll generate the certificate that the server itself will use. We include both `localhost`/`::1` as well as the
hostnames used by the integration tests, so the certificate should be usable in both scenarios.

```shell
# Make sure to change to the `tests/data/nats` directory first before running these commands.

# Create the server certificate/key. This will also generate a root CA which we'll need to copy as well.
$ mkcert -cert-file nats-server.pem -key-file nats-server.key localhost ::1 nats-tls nats-tls-client-cert nats-jwt

# Next, move the mkcert root CA to the correct location, and move the server certificate/key.
$ mv "$(mkcert -CAROOT)/rootCA.pem" tests/data/nats/rootCA.pem

# Now generate the client certificate/key.
$ mkcert -client -cert-file nats-client.pem -key-file nats-client.key localhost ::1 nats-tls nats-tls-client-cert nats-jwt email@localhost
```

After that, you can read more about [TLS configuration in
NATS](https://docs.nats.io/running-a-nats-service/configuration/securing_nats/tls) to learn what to add to the
configuration file itself. It also covers generating certificates using `mkcert`, but the above commands are specific to
our existing test configurations, so you can skip read the link,. and simply follow the commands, if all you're doing
is regenerating the certificates due to expiration, etc.

## Generating the JWT-based configuration credentials

You'll first need to follow the steps for [JWT authentication in
NATS](https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro/jwt/mem_resolver), which
requires first generating the necessary TLS certificates, so don't skip the above section.

Next, we'll need to move the credentials that the above guide had you generate.

```shell
# Now move these credentials to the right location, since they don't just drop into the current directory.
$ mv ~/.nkeys/creds/memory/A/TA.creds tests/data/nats/nats.creds

# After that, we make a copy that will act as the "bad" credentials.
#
# You'll need to open the "bad" version and change one of the characters in the seed value in order to actually make them "bad". :)
$ cp tests/data/nats/nats.creds tests/data/nats/nats-bad.creds
```
