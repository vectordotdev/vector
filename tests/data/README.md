# Test Data

* [`./GeoIP2-City-Test.mmdb`](https://github.com/maxmind/MaxMind-DB/tree/6e99232bb6a70d5169ecc96ed0614a52017ff654/test-data)
* `ca/private/ca.key.pem
  * `openssl genrsa  -out ca/private/ca.key.pem 4096`
* `ca/certs/ca.cert.pem
  *
  ```
  openssl req -config ca/openssl.cnf \
    -key ca/private/ca.key.pem \
    -new -x509 -days 7300 -sha256 -extensions v3_ca \
    -out ca/certs/ca.cert.pem -subj '/CN=Vector CA/OU=Vector/O=Datadog/ST=New York/L=New York/C=US'
    ```

* intermediate_server/private/intermediate_server.key.pem
* openssl genrsa \
      -out intermediate/private/intermediate.key.pem 4096

intermediate_server/csr/intermediate_server.csr.pem
* openssl req -config intermediate_server/openssl.cnf -new -sha256 \
      -key intermediate_server/private/intermediate_server.key.pem \
      -subj '/CN=Vector Intermediate Server CA/OU=Vector/O=Datadog/ST=New York/L=New York/C=US' \
      -out intermediate_server/csr/intermediate_server.csr.pem

intermediate_server/certs/intermediate_server.cert.pem
* openssl ca -config openssl.cnf -extensions v3_intermediate_ca \
      -days 3650 -notext -md sha256 \
      -in intermediate_server/csr/intermediate_server.csr.pem \
      -out intermediate_server/certs/intermediate_server.cert.pem

intermediate_server/certs/ca-chain.cert.pem
* cat intermediate_server/certs/intermediate_server.cert.pem \
      certs/ca.cert.pem > intermediate_server/certs/ca-chain.cert.pem

intermediate_server/private/localhost.key.pem
* openssl genrsa -out intermediate_server/private/localhost.key.pem 2048

intermediate_server/csr/localhost.csr.pem
* openssl req -config intermediate_server/openssl.cnf \
      -key intermediate_server/private/localhost.key.pem \
      -subj '/CN=localhost/OU=Vector/O=Datadog/ST=New York/L=New York/C=US' \
      -new -sha256 -out intermediate_server/csr/localhost.csr.pem

certs/localhost.cert.pem
* openssl ca -config openssl.cnf \
      -extensions server_cert -days 3650 -notext -md sha256 \
      -in csr/localhost.csr.pem \
      -out certs/localhost.cert.pem

certs/localhost-chain.cert.pem
* cat certs/localhost.cert.pem certs/ca-chain.cert.pem > certs/localhost-chain.cert.pem

Client

private/intermediate_client.key.pem
* openssl genrsa -out private/intermediate_client.key.pem 4096

csr/intermediate_client.csr.pem
* openssl req -config openssl.cnf -new -sha256 \
      -key private/intermediate_client.key.pem \
      -subj '/CN=Vector Intermediate Client CA/OU=Vector/O=Datadog/ST=New York/L=New York/C=US' \
      -out csr/intermediate_client.csr.pem

intermediate_client/certs/intermediate_client.cert.pem
* openssl ca -config openssl.cnf -extensions v3_intermediate_ca \
      -days 3650 -notext -md sha256 \
      -in intermediate_client/csr/intermediate_client.csr.pem \
      -out intermediate_client/certs/intermediate_client.cert.pem

intermediate_client/certs/ca-chain.cert.pem
 * cat intermediate_client/certs/intermediate_client.cert.pem \
      certs/ca.cert.pem > intermediate_client/certs/ca-chain.cert.pem

private/localhost.pem
* openssl genrsa -out private/localhost.pem 2048

csr/localhost.csr.pem
* openssl req -config openssl.cnf \
      -key private/localhost.key.pem \
      -subj '/CN=localhost/OU=Vector/O=Datadog/ST=New York/L=New York/C=US' \
      -new -sha256 -out csr/localhost.csr.pem

certs/localhost.cert.pem
* openssl ca -config openssl.cnf \
      -extensions usr_cert -days 375 -notext -md sha256 \
      -in csr/localhost.csr.pem \
      -out certs/localhost.cert.pem

certs/localhost-chain.cert.pem
* cat certs/localhost.cert.pem certs/ca-chain.cert.pem > certs/localhost-chain.cert.pem
