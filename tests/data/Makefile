DAYS = 9999
BITS = 4096
CA_BASE = Vector_CA
CRT_BASE = localhost

all: $(CRT_BASE).crt $(CRT_BASE).p12

clean:
	rm -f $(CRT_BASE).crt $(CRT_BASE).key $(CRT_BASE).p12
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

