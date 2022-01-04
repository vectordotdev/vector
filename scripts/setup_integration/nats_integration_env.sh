#!/usr/bin/env bash
set -o pipefail

# nats_integration_env.sh
#
# SUMMARY
#
#   Builds and pulls down the Vector NATS Integration test environment

if [ $# -ne 1 ]
then
    echo "Usage: $0 {stop|start}" 1>&2; exit 1;
    exit 1
fi
ACTION=$1


start_podman () {
  podman pod create --replace --name vector_nats -p 4222:4222
  podman pod create --replace --name vector_nats_userpass -p 4223:4222
  podman pod create --replace --name vector_nats_token -p 4224:4222
  podman pod create --replace --name vector_nats_nkey -p 4225:4222
  podman pod create --replace --name vector_nats_tls -p 4226:4222
  podman pod create --replace --name vector_nats_tls_client_cert -p 4228:4222
  podman pod create --replace --name vector_nats_jwt -p 4229:4222

  podman run -d --pod=vector_nats --name vector_nats_test nats
  podman run -d --pod=vector_nats_userpass --name vector_nats_userpass_test nats \
      --user natsuser --pass natspass
  podman run -d --pod=vector_nats_token --name vector_nats_token_test nats \
      --auth secret
  podman run -d --pod=vector_nats_nkey --name vector_nats_nkey_test \
    -v "$(pwd)"/tests/data:/usr/share/nats/config:ro \
    nats -c /usr/share/nats/config/nats-nkey.conf

  podman run -d --pod=vector_nats_tls --name vector_nats_tls_test \
    -v "$(pwd)"/tests/data:/usr/share/nats/config:ro \
    nats -c /usr/share/nats/config/nats-tls.conf

  podman run -d --pod=vector_nats_tls_client_cert --name vector_nats_tls_client_cert_test \
    -v "$(pwd)"/tests/data:/usr/share/nats/config:ro \
    nats -c /usr/share/nats/config/nats-tls-client-cert.conf

  podman run -d --pod=vector_nats_jwt --name vector_nats_jwt_test \
    -v "$(pwd)"/tests/data:/usr/share/nats/config:ro \
    nats -c /usr/share/nats/config/nats-jwt.conf
}

start_docker () {
  docker network create vector-test-integration-nats
  docker run -d --network=vector-test-integration-nats -p 4222:4222 --name vector_nats nats
  docker run -d --network=vector-test-integration-nats -p 4223:4222 --name vector_nats_userpass nats \
    --user natsuser --pass natspass
  docker run -d --network=vector-test-integration-nats -p 4224:4222 --name vector_nats_token nats \
    --auth secret

  # The following tls tests use mkcert
  # https://github.com/FiloSottile/mkcert
  # See https://docs.nats.io/running-a-nats-service/configuration/securing_nats/tls


  # Generate a new NKey with the following command:
  # $ nk -gen user -pubout
  # SUANIRXEZUROTXNFN3TJYMT27K7ZZVMD46FRIHF6KXKS4KGNVBS57YAFGY
  # UD345ZYSUJQD7PNCTWQPINYSO3VH4JBSADBSYUZOBT666DRASFRAWAWT
  #
  # The first line of output is the Seed, which is a private key
  # The second line of output is the User string, which is a public key
  docker run -d --network=vector-test-integration-nats -p 4225:4222 \
    -v "$(pwd)"/tests/data:/usr/share/nats/config:ro \
    --name vector_nats_nkey nats \
    -c /usr/share/nats/config/nats-nkey.conf

  # First, generate a certificate for the NATS server using the following command
  # $ mkcert -cert-file server-cert.pem -key-file server-key.pem localhost ::1
  #
  # Next, move the mkcert root CA to the correct location
  # $ mv "$(mkcert -CAROOT)/rootCA.pem" tests/data/mkcert_rootCA.pem
  docker run -d --network=vector-test-integration-nats -p 4227:4222 \
    -v "$(pwd)"/tests/data:/usr/share/nats/config:ro \
    --name vector_nats_tls nats \
    -c /usr/share/nats/config/nats-tls.conf

  # Generate a client cert using the following command
  # $ mkcert -client -cert-file nats_client_cert.pem -key-file nats_client_key.pem localhost ::1 email@localhost
  docker run -d --network=vector-test-integration-nats -p 4228:4222 \
    -v "$(pwd)"/tests/data:/usr/share/nats/config:ro \
    --name vector_nats_tls_client_cert nats \
    -c /usr/share/nats/config/nats-tls-client-cert.conf

  # Follow the instructions here
  # See https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro/jwt/mem_resolver
  # Then run the following additional commands
  # $ mv /tmp/server.conf tests/data/nats-jwt.conf
  # $ cat << EOF >> tests/data/nats-jwt.conf
  #
  #tls: {
  #  cert_file: "/usr/share/nats/config/localhost-mkcert.pem"
  #  key_file: "/usr/share/nats/config/localhost-mkcert-key.pem"
  #}
  #EOF
  #
  # $ mv ~/.nkeys/creds/memory/A/TA.creds tests/data/nats.creds
  # $ cp tests/data/nats.creds tests/data/nats-bad.creds
  # # edit test/data/nats-bad.creds and change one of the characters in the Seed

  docker run -d --network=vector-test-integration-nats -p 4229:4222 \
    -v "$(pwd)"/tests/data:/usr/share/nats/config:ro \
    --name vector_nats_jwt nats \
    -c /usr/share/nats/config/nats-jwt.conf
}

stop_podman () {
  podman pod stop vector_nats_test 2>/dev/null; true
  podman pod rm --force vector_nats 2>/dev/null; true

  podman pod stop vector_nats_userpass_test 2>/dev/null; true
  podman pod rm --force vector_nats_userpass 2>/dev/null; true

  podman pod stop vector_nats_token_test 2>/dev/null; true
  podman pod rm --force vector_nats_token 2>/dev/null; true

  podman pod stop vector_nats_tls_test 2>/dev/null; true
  podman pod rm --force vector_nats_tls 2>/dev/null; true

  podman pod stop vector_nats_tls_client_cert_test 2>/dev/null; true
  podman pod rm --force vector_nats_tls_client_cert 2>/dev/null; true

  podman pod stop vector_nats_jwt_test 2>/dev/null; true
  podman pod rm --force vector_nats_jwt 2>/dev/null; true
}

stop_docker () {
  docker rm --force vector_nats 2>/dev/null; true
  docker rm --force vector_nats_userpass 2>/dev/null; true
  docker rm --force vector_nats_token 2>/dev/null; true
  docker rm --force vector_nats_nkey 2>/dev/null; true
  docker rm --force vector_nats_tls 2>/dev/null; true
  docker rm --force vector_nats_tls_client_cert 2>/dev/null; true
  docker rm --force vector_nats_jwt 2>/dev/null; true
  docker network rm vector-test-integration-nats 2>/dev/null; true
}

echo "Running $ACTION action for NATS integration tests environment"

"${ACTION}"_"${CONTAINER_TOOL}"
