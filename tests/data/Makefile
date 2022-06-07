all: ca/intermediate_server/certs/localhost-chain.cert.pem ca/intermediate_client/certs/localhost-chain.cert.pem ca/intermediate_client/private/localhost.p12

clean:
	rm -rf ca/private ca/certs ca/newcerts ca/crl ca/index.txt
	rm -rf ca/intermediate_server/private ca/intermediate_server/certs ca/intermediate_server/newcerts ca/intermediate_server/crl ca/intermediate_server/index.txt
	rm -rf ca/intermediate_client/private ca/intermediate_client/certs ca/intermediate_client/newcerts ca/intermediate_client/crl ca/intermediate_client/index.txt

ca/private/ca.key.pem:
	mkdir -p ca/private
	openssl genrsa -out ca/private/ca.key.pem 4096

ca/certs/ca.cert.pem: ca/private/ca.key.pem
	mkdir -p ca/certs ca/crl ca/newcerts
	touch ca/index.txt
	echo 1000 > ca/serial
	openssl req -config ca/openssl.cnf \
		-key ca/private/ca.key.pem \
		-new -x509 -days 7300 -sha256 -extensions v3_ca \
		-subj '/CN=Vector CA/OU=Vector/O=Datadog/ST=New York/L=New York/C=US' \
		-out ca/certs/ca.cert.pem

ca/intermediate_server/private/intermediate_server.key.pem:
	mkdir -p ca/intermediate_server/private
	openssl genrsa -out ca/intermediate_server/private/intermediate_server.key.pem 4096

ca/intermediate_server/csr/intermediate_server.csr.pem: ca/intermediate_server/private/intermediate_server.key.pem
	mkdir -p ca/intermediate_server/csr
	openssl req -config ca/intermediate_server/openssl.cnf -new -sha256 \
      -key ca/intermediate_server/private/intermediate_server.key.pem \
      -subj '/CN=Vector Intermediate Server CA/OU=Vector/O=Datadog/ST=New York/L=New York/C=US' \
      -out ca/intermediate_server/csr/intermediate_server.csr.pem

ca/intermediate_server/certs/intermediate_server.cert.pem: ca/intermediate_server/csr/intermediate_server.csr.pem
	mkdir -p ca/intermediate_server/certs ca/intermediate_server/crl ca/intermediate_server/newcerts
	touch ca/intermediate_server/index.txt
	echo 1000 > ca/intermediate_server/serial
	openssl ca -batch -config ca/openssl.cnf -extensions v3_intermediate_ca \
		-days 3650 -notext -md sha256 \
		-in ca/intermediate_server/csr/intermediate_server.csr.pem \
		-out ca/intermediate_server/certs/intermediate_server.cert.pem

ca/intermediate_server/certs/ca-chain.cert.pem: ca/certs/ca.cert.pem ca/intermediate_server/certs/intermediate_server.cert.pem
	cat ca/intermediate_server/certs/intermediate_server.cert.pem \
		ca/certs/ca.cert.pem > ca/intermediate_server/certs/ca-chain.cert.pem

ca/intermediate_server/private/localhost.key.pem:
	openssl genrsa -out ca/intermediate_server/private/localhost.key.pem 2048

ca/intermediate_server/csr/localhost.csr.pem: ca/intermediate_server/private/localhost.key.pem
	openssl req -config ca/intermediate_server/openssl.cnf \
		-key ca/intermediate_server/private/localhost.key.pem \
		-subj '/CN=localhost/OU=Vector/O=Datadog/ST=New York/L=New York/C=US' \
		-new -sha256 -out ca/intermediate_server/csr/localhost.csr.pem

ca/intermediate_server/certs/localhost.cert.pem: ca/intermediate_server/csr/localhost.csr.pem
	openssl ca -batch -config ca/intermediate_server/openssl.cnf \
		-extensions server_cert -days 3650 -notext -md sha256 \
		-in ca/intermediate_server/csr/localhost.csr.pem \
		-out ca/intermediate_server/certs/localhost.cert.pem

ca/intermediate_server/certs/localhost-chain.cert.pem: ca/intermediate_server/certs/ca-chain.cert.pem ca/intermediate_server/certs/localhost.cert.pem
	cat ca/intermediate_server/certs/localhost.cert.pem ca/intermediate_server/certs/ca-chain.cert.pem > ca/intermediate_server/certs/localhost-chain.cert.pem

ca/intermediate_client/private/localhost.p12: ca/intermediate_server/private/localhost.key.pem ca/intermediate_server/certs/localhost.cert.pem
	openssl pkcs12 -export -password pass:NOPASS \
		-out ca/intermediate_client/private/localhost.p12 \
		-in ca/intermediate_server/certs/localhost.cert.pem \
		-inkey ca/intermediate_server/private/localhost.key.pem

ca/intermediate_client/private/intermediate_client.key.pem:
	mkdir -p ca/intermediate_client/private
	openssl genrsa -out ca/intermediate_client/private/intermediate_client.key.pem 4096

ca/intermediate_client/csr/intermediate_client.csr.pem: ca/intermediate_client/private/intermediate_client.key.pem
	mkdir -p ca/intermediate_client/csr
	openssl req -config ca/intermediate_client/openssl.cnf -new -sha256 \
      -key ca/intermediate_client/private/intermediate_client.key.pem \
      -subj '/CN=Vector Intermediate Client CA/OU=Vector/O=Datadog/ST=New York/L=New York/C=US' \
      -out ca/intermediate_client/csr/intermediate_client.csr.pem

ca/intermediate_client/certs/intermediate_client.cert.pem: ca/intermediate_client/csr/intermediate_client.csr.pem
	mkdir -p ca/intermediate_client/certs ca/intermediate_client/crl ca/intermediate_client/newcerts
	touch ca/intermediate_client/index.txt
	echo 1000 > ca/intermediate_client/serial
	openssl ca -batch -config ca/openssl.cnf -extensions v3_intermediate_ca \
		-days 3650 -notext -md sha256 \
		-in ca/intermediate_client/csr/intermediate_client.csr.pem \
		-out ca/intermediate_client/certs/intermediate_client.cert.pem

ca/intermediate_client/certs/ca-chain.cert.pem: ca/certs/ca.cert.pem ca/intermediate_client/certs/intermediate_client.cert.pem
	cat ca/intermediate_client/certs/intermediate_client.cert.pem \
		ca/certs/ca.cert.pem > ca/intermediate_client/certs/ca-chain.cert.pem

ca/intermediate_client/private/localhost.key.pem:
	openssl genrsa -out ca/intermediate_client/private/localhost.key.pem 2048

ca/intermediate_client/csr/localhost.csr.pem: ca/intermediate_client/private/localhost.key.pem
	openssl req -config ca/intermediate_client/openssl.cnf \
		-key ca/intermediate_client/private/localhost.key.pem \
		-subj '/CN=localhost/OU=Vector/O=Datadog/ST=New York/L=New York/C=US' \
		-new -sha256 -out ca/intermediate_client/csr/localhost.csr.pem

ca/intermediate_client/certs/localhost.cert.pem: ca/intermediate_client/csr/localhost.csr.pem
	openssl ca -batch -config ca/intermediate_client/openssl.cnf \
		-extensions usr_cert -days 3650 -notext -md sha256 \
		-in ca/intermediate_client/csr/localhost.csr.pem \
		-out ca/intermediate_client/certs/localhost.cert.pem

ca/intermediate_client/certs/localhost-chain.cert.pem: ca/intermediate_client/certs/ca-chain.cert.pem ca/intermediate_client/certs/localhost.cert.pem
	cat ca/intermediate_client/certs/localhost.cert.pem ca/intermediate_client/certs/ca-chain.cert.pem > ca/intermediate_client/certs/localhost-chain.cert.pem
