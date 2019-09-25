DAYS = 9999
BITS = 4096
CA_BASE = Vector_CA
CRT_BASE = localhost

all: $(CRT_BASE).crt $(CRT_BASE).jks

clean:
	rm -f $(CRT_BASE).crt $(CRT_BASE).key $(CRT_BASE).jks
	rm -f $(CA_BASE).crt $(CA_BASE).key $(CA_BASE).srl

%.key:
	openssl genrsa -out $@ $(BITS)

%.csr: %.cfg %.key
	openssl req -new -config $*.cfg -key $*.key -days $(DAYS) -out $@

%.crt: %.csr $(CA_BASE).crt $(CA_BASE).key
	openssl x509 -req -in $*.csr -CA $(CA_BASE).crt -CAkey $(CA_BASE).key -CAcreateserial -days $(DAYS) -out $@
	@rm $*.csr $(CA_BASE).srl

$(CA_BASE).crt: $(CA_BASE).cfg $(CA_BASE).key
	openssl req -x509 -new -config $(CA_BASE).cfg -key $(CA_BASE).key -days $(DAYS) -out $@

%.p12: %.key %.crt
	openssl pkcs12 -export -inkey $*.key -in $*.crt -name $* -password pass:NOPASS -out $@

%.jks: %.p12
	@rm -f $@
	docker run --rm --volume=$(CURDIR):/certs wurstmeister/kafka \
		keytool -importkeystore \
		-srckeystore /certs/$*.p12 \
		-srcstoretype pkcs12 \
		-destkeypass NOPASS \
		-srcstorepass NOPASS \
		-destkeystore /certs/$@ \
		-storepass NOPASS
	@# Output file will be owned as root, fix ownership
	@mv -f $@ $@.root
	@cp -f $@.root $@
	@rm -f $@.root
	@rm $*.p12
