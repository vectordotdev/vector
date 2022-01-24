DAYS = 9999
BITS = 4096
CA_BASE = Vector_CA
CA0_BASE = Dummy_CA
CRT_BASE? = localhost
MULTI_CA = Multi_CA.crt
INTERMEDIATE_BASE = Intermediate_CA
CRT_FROM_INTERMEDIATE_BASE = Crt_from_intermediate
CHAIN_WITH_INTERMEDIATE = Chain_with_intermediate.crt

all: $(CRT_BASE).crt $(CRT_BASE).key $(CRT_BASE).p12 $(MULTI_CA) $(CRT_FROM_INTERMEDIATE_BASE).crt $(CHAIN_WITH_INTERMEDIATE)

clean:
	rm -f $(CRT_BASE).crt $(CRT_BASE).key $(CRT_BASE).p12
	rm -f $(CA_BASE).crt $(CA_BASE).key $(CA_BASE).srl
	rm -f $(CA0_BASE).crt $(CA0_BASE).key $(CA0_BASE).srl
	rm -f $(MULTI_CA) $(CHAIN_WITH_INTERMEDIATE)
	rm -f $(INTERMEDIATE_BASE).crt $(INTERMEDIATE_BASE).key $(INTERMEDIATE_BASE).csr $(INTERMEDIATE_BASE).srl
	rm -f $(CRT_FROM_INTERMEDIATE_BASE).crt $(CRT_FROM_INTERMEDIATE_BASE).key
	rm -rf certs/

$(MULTI_CA): $(CA0_BASE).crt $(CA_BASE).crt
	cat $^ > $@

%.key:
	openssl genrsa -out $@ $(BITS)

%.csr: %.cfg %.key
	openssl req -new -config $*.cfg -key $*.key -days $(DAYS) -out $@

%.crt: %.csr $(CA_BASE).crt $(CA_BASE).key
	openssl x509 -req -in $*.csr -CA $(CA_BASE).crt -CAkey $(CA_BASE).key -CAcreateserial -days $(DAYS) -out $@
	@rm $*.csr $(CA_BASE).srl

$(INTERMEDIATE_BASE).crt: $(INTERMEDIATE_BASE).cfg $(INTERMEDIATE_BASE).key
	mkdir -p certs/new
	touch certs/database
	echo "1000" > certs/serial
	openssl req -new -config $(INTERMEDIATE_BASE).cfg -key $(INTERMEDIATE_BASE).key -days $(DAYS) -out $(INTERMEDIATE_BASE).csr
	openssl ca -batch -config $(CA_BASE).cfg -in $(INTERMEDIATE_BASE).csr -out $@

$(CRT_FROM_INTERMEDIATE_BASE).csr: $(CRT_FROM_INTERMEDIATE_BASE).cfg $(CRT_FROM_INTERMEDIATE_BASE).key
	openssl req -new -config $(CRT_FROM_INTERMEDIATE_BASE).cfg -key $(CRT_FROM_INTERMEDIATE_BASE).key -days $(DAYS) -out $@

$(CRT_FROM_INTERMEDIATE_BASE).crt: $(INTERMEDIATE_BASE).crt $(INTERMEDIATE_BASE).key $(CA_BASE).crt $(CRT_FROM_INTERMEDIATE_BASE).csr $(CRT_FROM_INTERMEDIATE_BASE).key
	openssl x509 -req -in $(CRT_FROM_INTERMEDIATE_BASE).csr -CA $(INTERMEDIATE_BASE).crt -CAkey $(INTERMEDIATE_BASE).key -CAcreateserial -days $(DAYS) -out $@

$(CHAIN_WITH_INTERMEDIATE): $(CRT_FROM_INTERMEDIATE_BASE).crt $(INTERMEDIATE_BASE).crt $(CA_BASE).crt
	cat $^ > $@

$(CA_BASE).crt: $(CA_BASE).cfg $(CA_BASE).key
	openssl req -x509 -new -config $(CA_BASE).cfg -key $(CA_BASE).key -days $(DAYS) -out $@

$(CA0_BASE).crt: $(CA0_BASE).cfg $(CA0_BASE).key
	openssl req -x509 -new -config $(CA0_BASE).cfg -key $(CA0_BASE).key -days $(DAYS) -out $@

%.p12: %.key %.crt
	openssl pkcs12 -export -inkey $*.key -in $*.crt -name $* -password pass:NOPASS -out $@

