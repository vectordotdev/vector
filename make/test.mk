##@ Testing (Supports `ENVIRONMENT=true`)

.PHONY: test
test: ## Run the unit test suite
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --workspace --features ${DEFAULT_FEATURES} ${SCOPE} --all-targets -- --nocapture

.PHONY: test-components
test-components: ## Test with all components enabled
test-components: $(WASM_MODULE_OUTPUTS)
# TODO(jesse) add `wasm-benches` when https://github.com/timberio/vector/issues/5106 is fixed
test-components: export DEFAULT_FEATURES:="${DEFAULT_FEATURES} benches"
test-components: test

.PHONY: test-all
test-all: test test-behavior test-integration ## Runs all tests, unit, behaviorial, and integration.

.PHONY: test-x86_64-unknown-linux-gnu
test-x86_64-unknown-linux-gnu: cross-test-x86_64-unknown-linux-gnu ## Runs unit tests on the x86_64-unknown-linux-gnu triple
	${EMPTY}

.PHONY: test-aarch64-unknown-linux-gnu
test-aarch64-unknown-linux-gnu: cross-test-aarch64-unknown-linux-gnu ## Runs unit tests on the aarch64-unknown-linux-gnu triple
	${EMPTY}

.PHONY: test-behavior
test-behavior: ## Runs behaviorial test
	${MAYBE_ENVIRONMENT_EXEC} cargo run -- test tests/behavior/**/*

.PHONY: test-integration
test-integration: ## Runs all integration tests
test-integration: test-integration-aws test-integration-clickhouse test-integration-docker-logs test-integration-elasticsearch
test-integration: test-integration-gcp test-integration-humio test-integration-influxdb test-integration-kafka
test-integration: test-integration-loki test-integration-mongodb_metrics test-integration-nats
test-integration: test-integration-nginx test-integration-prometheus test-integration-pulsar test-integration-splunk

.PHONY: start-test-integration
start-test-integration: ## Starts all integration test infrastructure
start-test-integration: start-integration-aws start-integration-clickhouse start-integration-elasticsearch
start-test-integration: start-integration-gcp start-integration-humio start-integration-influxdb start-integration-kafka
start-test-integration: start-integration-loki start-integration-mongodb_metrics start-integration-nats
start-test-integration: start-integration-nginx start-integration-prometheus start-integration-pulsar start-integration-splunk

.PHONY: stop-test-integration
stop-test-integration: ## Stops all integration test infrastructure
stop-test-integration: stop-integration-aws stop-integration-clickhouse stop-integration-elasticsearch
stop-test-integration: stop-integration-gcp stop-integration-humio stop-integration-influxdb stop-integration-kafka
stop-test-integration: stop-integration-loki stop-integration-mongodb_metrics stop-integration-nats
stop-test-integration: stop-integration-nginx stop-integration-prometheus stop-integration-pulsar stop-integration-splunk

.PHONY: start-integration-aws
start-integration-aws:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-aws -p 4566:4566 -p 4571:4571 -p 6000:6000 -p 9088:80
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws --name vector_ec2_metadata \
	 timberiodev/mock-ec2-metadata:latest
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws --name vector_localstack_aws \
	 -e SERVICES=kinesis,s3,cloudwatch,elasticsearch,es,firehose,sqs \
	 localstack/localstack-full:0.11.6
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws --name vector_mockwatchlogs \
	 -e RUST_LOG=trace luciofranco/mockwatchlogs:latest
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws -v /var/run:/var/run --name vector_local_ecs \
	 -e RUST_LOG=trace amazon/amazon-ecs-local-container-endpoints:latest
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-aws
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws -p 8111:8111 --name vector_ec2_metadata \
	 timberiodev/mock-ec2-metadata:latest
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws --name vector_localstack_aws \
	 -p 4566:4566 -p 4571:4571 \
	 -e SERVICES=kinesis,s3,cloudwatch,elasticsearch,es,firehose,sqs \
	 localstack/localstack-full:0.11.6
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws -p 6000:6000 --name vector_mockwatchlogs \
	 -e RUST_LOG=trace luciofranco/mockwatchlogs:latest
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws -v /var/run:/var/run -p 9088:80 --name vector_local_ecs \
	 -e RUST_LOG=trace amazon/amazon-ecs-local-container-endpoints:latest
endif

.PHONY: stop-integration-aws
stop-integration-aws:
	$(CONTAINER_TOOL) rm --force vector_ec2_metadata vector_mockwatchlogs vector_localstack_aws vector_local_ecs 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-aws 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name=vector-test-integration-aws 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-aws 2>/dev/null; true
endif

.PHONY: test-integration-aws
test-integration-aws: ## Runs AWS integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-aws
	$(MAKE) start-integration-aws
	sleep 10 # Many services are very slow... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features aws-integration-tests --lib ::aws_ -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-aws
endif

.PHONY: start-integration-clickhouse
start-integration-clickhouse:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-clickhouse -p 8123:8123
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-clickhouse --name vector_clickhouse yandex/clickhouse-server:19
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-clickhouse
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-clickhouse -p 8123:8123 --name vector_clickhouse yandex/clickhouse-server:19
endif

.PHONY: stop-integration-clickhouse
stop-integration-clickhouse:
	$(CONTAINER_TOOL) rm --force vector_clickhouse 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-clickhouse 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-clickhouse 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-clickhouse 2>/dev/null; true
endif

.PHONY: test-integration-clickhouse
test-integration-clickhouse: ## Runs Clickhouse integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-clickhouse
	$(MAKE) start-integration-clickhouse
	sleep 5 # Many services are very slow... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features clickhouse-integration-tests --lib ::clickhouse:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-clickhouse
endif

.PHONY: test-integration-docker-logs
test-integration-docker-logs: ## Runs Docker Logs integration tests
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features docker-logs-integration-tests --lib ::docker_logs:: -- --nocapture

.PHONY: start-integration-elasticsearch
start-integration-elasticsearch:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-elasticsearch -p 4571:4571 -p 9200:9200 -p 9300:9300 -p 9201:9200 -p 9301:9300
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-elasticsearch --name vector_localstack_es \
	 -e SERVICES=elasticsearch:4571 localstack/localstack@sha256:f21f1fc770ee4bfd5012afdc902154c56b7fb18c14cf672de151b65569c8251e
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-elasticsearch \
	 --name vector_elasticsearch -e discovery.type=single-node -e ES_JAVA_OPTS="-Xms400m -Xmx400m" elasticsearch:6.6.2
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-elasticsearch \
	 --name vector_elasticsearch-tls -e discovery.type=single-node -e xpack.security.enabled=true \
	 -e xpack.security.http.ssl.enabled=true -e xpack.security.transport.ssl.enabled=true \
	 -e xpack.ssl.certificate=certs/localhost.crt -e xpack.ssl.key=certs/localhost.key \
	 -e ES_JAVA_OPTS="-Xms400m -Xmx400m" \
	 -v $(PWD)/tests/data:/usr/share/elasticsearch/config/certs:ro elasticsearch:6.6.2
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-elasticsearch
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-elasticsearch -p 4571:4571 --name vector_localstack_es \
	 -e SERVICES=elasticsearch:4571 localstack/localstack@sha256:f21f1fc770ee4bfd5012afdc902154c56b7fb18c14cf672de151b65569c8251e
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-elasticsearch -p 9200:9200 -p 9300:9300 \
	 --name vector_elasticsearch -e discovery.type=single-node -e ES_JAVA_OPTS="-Xms400m -Xmx400m" elasticsearch:6.6.2
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-elasticsearch -p 9201:9200 -p 9301:9300 \
	 --name vector_elasticsearch-tls -e discovery.type=single-node -e xpack.security.enabled=true \
	 -e xpack.security.http.ssl.enabled=true -e xpack.security.transport.ssl.enabled=true \
	 -e xpack.ssl.certificate=certs/localhost.crt -e xpack.ssl.key=certs/localhost.key \
	 -e ES_JAVA_OPTS="-Xms400m -Xmx400m" \
	 -v $(PWD)/tests/data:/usr/share/elasticsearch/config/certs:ro elasticsearch:6.6.2
endif

.PHONY: stop-integration-elasticsearch
stop-integration-elasticsearch:
	$(CONTAINER_TOOL) rm --force vector_localstack_es vector_elasticsearch vector_elasticsearch-tls 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-elasticsearch 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-elasticsearch 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-elasticsearch 2>/dev/null; true
endif

.PHONY: test-integration-elasticsearch
test-integration-elasticsearch: ## Runs Elasticsearch integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-elasticsearch
	$(MAKE) start-integration-elasticsearch
	sleep 60 # Many services are very slow... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features es-integration-tests --lib ::elasticsearch:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-elasticsearch
endif

.PHONY: start-integration-gcp
start-integration-gcp:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-gcp -p 8681-8682:8681-8682
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-gcp --name vector_cloud-pubsub \
	 -e PUBSUB_PROJECT1=testproject,topic1:subscription1 messagebird/gcloud-pubsub-emulator
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-gcp
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-gcp -p 8681-8682:8681-8682 --name vector_cloud-pubsub \
	 -e PUBSUB_PROJECT1=testproject,topic1:subscription1 messagebird/gcloud-pubsub-emulator
endif

.PHONY: stop-integration-gcp
stop-integration-gcp:
	$(CONTAINER_TOOL) rm --force vector_cloud-pubsub 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-gcp 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-gcp 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-gcp 2>/dev/null; true
endif

.PHONY: test-integration-gcp
test-integration-gcp: ## Runs GCP integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-gcp
	$(MAKE) start-integration-gcp
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features "gcp-integration-tests gcp-pubsub-integration-tests gcp-cloud-storage-integration-tests" \
	 --lib ::gcp:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-gcp
endif

.PHONY: start-integration-humio
start-integration-humio:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-humio -p 8080:8080
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-humio --name vector_humio humio/humio:1.13.1
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-humio
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-humio -p 8080:8080 --name vector_humio humio/humio:1.13.1
endif

.PHONY: stop-integration-humio
stop-integration-humio:
	$(CONTAINER_TOOL) rm --force vector_humio 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-humio 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-humio 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-humio 2>/dev/null; true
endif

.PHONY: test-integration-humio
test-integration-humio: ## Runs Humio integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-humio
	$(MAKE) start-integration-humio
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features humio-integration-tests --lib "::humio::.*::integration_tests::" -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-humio
endif

.PHONY: start-integration-influxdb
start-integration-influxdb:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-influxdb -p 8086:8086 -p 8087:8087 -p 9999:9999
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-influxdb --name vector_influxdb_v1 \
	 -e INFLUXDB_REPORTING_DISABLED=true influxdb:1.8
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-influxdb --name vector_influxdb_v1_tls \
	 -e INFLUXDB_REPORTING_DISABLED=true -e INFLUXDB_HTTP_HTTPS_ENABLED=true -e INFLUXDB_HTTP_BIND_ADDRESS=:8087 -e INFLUXDB_BIND_ADDRESS=:8089 \
	 -e INFLUXDB_HTTP_HTTPS_CERTIFICATE=/etc/ssl/localhost.crt -e INFLUXDB_HTTP_HTTPS_PRIVATE_KEY=/etc/ssl/localhost.key \
	 -v $(PWD)/tests/data:/etc/ssl:ro influxdb:1.8
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-influxdb --name vector_influxdb_v2 \
	 -e INFLUXDB_REPORTING_DISABLED=true  quay.io/influxdb/influxdb:2.0.0-rc influxd --reporting-disabled --http-bind-address=:9999
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-influxdb
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-influxdb -p 8086:8086 --name vector_influxdb_v1 \
	 -e INFLUXDB_REPORTING_DISABLED=true influxdb:1.8
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-influxdb -p 8087:8087 --name vector_influxdb_v1_tls \
	 -e INFLUXDB_REPORTING_DISABLED=true -e INFLUXDB_HTTP_HTTPS_ENABLED=true -e INFLUXDB_HTTP_BIND_ADDRESS=:8087 \
	 -e INFLUXDB_HTTP_HTTPS_CERTIFICATE=/etc/ssl/localhost.crt -e INFLUXDB_HTTP_HTTPS_PRIVATE_KEY=/etc/ssl/localhost.key \
	 -v $(PWD)/tests/data:/etc/ssl:ro influxdb:1.8
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-influxdb -p 9999:9999 --name vector_influxdb_v2 \
	 -e INFLUXDB_REPORTING_DISABLED=true  quay.io/influxdb/influxdb:2.0.0-rc influxd --reporting-disabled --http-bind-address=:9999
endif

.PHONY: stop-integration-influxdb
stop-integration-influxdb:
	$(CONTAINER_TOOL) rm --force vector_influxdb_v1 vector_influxdb_v1_tls vector_influxdb_v2 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-influxdb 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-influxdb 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-influxdb 2>/dev/null; true
endif

.PHONY: test-integration-influxdb
test-integration-influxdb: ## Runs InfluxDB integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-influxdb
	$(MAKE) start-integration-influxdb
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features influxdb-integration-tests --lib integration_tests:: --  ::influxdb --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-influxdb
endif

.PHONY: start-integration-kafka
start-integration-kafka:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-kafka -p 2181:2181 -p 9091-9093:9091-9093
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-kafka --name vector_zookeeper wurstmeister/zookeeper
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-kafka -e KAFKA_BROKER_ID=1 \
	 -e KAFKA_ZOOKEEPER_CONNECT=vector_zookeeper:2181 -e KAFKA_LISTENERS=PLAINTEXT://:9091,SSL://:9092,SASL_PLAINTEXT://:9093 \
	 -e KAFKA_ADVERTISED_LISTENERS=PLAINTEXT://localhost:9091,SSL://localhost:9092,SASL_PLAINTEXT://localhost:9093 \
	 -e KAFKA_SSL_KEYSTORE_LOCATION=/certs/localhost.p12 -e KAFKA_SSL_KEYSTORE_PASSWORD=NOPASS \
	 -e KAFKA_SSL_TRUSTSTORE_LOCATION=/certs/localhost.p12 -e KAFKA_SSL_TRUSTSTORE_PASSWORD=NOPASS \
	 -e KAFKA_SSL_KEY_PASSWORD=NOPASS -e KAFKA_SSL_ENDPOINT_IDENTIFICATION_ALGORITHM=none \
	 -e KAFKA_OPTS="-Djava.security.auth.login.config=/etc/kafka/kafka_server_jaas.conf" \
	 -e KAFKA_INTER_BROKER_LISTENER_NAME=SASL_PLAINTEXT -e KAFKA_SASL_ENABLED_MECHANISMS=PLAIN \
	 -e KAFKA_SASL_MECHANISM_INTER_BROKER_PROTOCOL=PLAIN -v $(PWD)/tests/data/localhost.p12:/certs/localhost.p12:ro \
	 -v $(PWD)/tests/data/kafka_server_jaas.conf:/etc/kafka/kafka_server_jaas.conf --name vector_kafka wurstmeister/kafka
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-kafka
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-kafka -p 2181:2181 --name vector_zookeeper wurstmeister/zookeeper
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-kafka -p 9091-9093:9091-9093 -e KAFKA_BROKER_ID=1 \
	 -e KAFKA_ZOOKEEPER_CONNECT=vector_zookeeper:2181 -e KAFKA_LISTENERS=PLAINTEXT://:9091,SSL://:9092,SASL_PLAINTEXT://:9093 \
	 -e KAFKA_ADVERTISED_LISTENERS=PLAINTEXT://localhost:9091,SSL://localhost:9092,SASL_PLAINTEXT://localhost:9093 \
	 -e KAFKA_SSL_KEYSTORE_LOCATION=/certs/localhost.p12 -e KAFKA_SSL_KEYSTORE_PASSWORD=NOPASS \
	 -e KAFKA_SSL_TRUSTSTORE_LOCATION=/certs/localhost.p12 -e KAFKA_SSL_TRUSTSTORE_PASSWORD=NOPASS \
	 -e KAFKA_SSL_KEY_PASSWORD=NOPASS -e KAFKA_SSL_ENDPOINT_IDENTIFICATION_ALGORITHM=none \
	 -e KAFKA_OPTS="-Djava.security.auth.login.config=/etc/kafka/kafka_server_jaas.conf" \
	 -e KAFKA_INTER_BROKER_LISTENER_NAME=SASL_PLAINTEXT -e KAFKA_SASL_ENABLED_MECHANISMS=PLAIN \
	 -e KAFKA_SASL_MECHANISM_INTER_BROKER_PROTOCOL=PLAIN -v $(PWD)/tests/data/localhost.p12:/certs/localhost.p12:ro \
	 -v $(PWD)/tests/data/kafka_server_jaas.conf:/etc/kafka/kafka_server_jaas.conf --name vector_kafka wurstmeister/kafka
endif

.PHONY: stop-integration-kafka
stop-integration-kafka:
	$(CONTAINER_TOOL) rm --force vector_kafka vector_zookeeper 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-kafka 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-kafka 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-kafka 2>/dev/null; true
endif

.PHONY: test-integration-kafka
test-integration-kafka: ## Runs Kafka integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-kafka
	$(MAKE) start-integration-kafka
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features "kafka-integration-tests rdkafka-plain" --lib ::kafka:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-kafka
endif

.PHONY: start-integration-loki
start-integration-loki:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-loki -p 3100:3100
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-loki -v $(PWD)/tests/data:/etc/loki \
	 --name vector_loki grafana/loki:master -config.file=/etc/loki/loki-config.yaml
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-loki
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-loki -p 3100:3100 -v $(PWD)/tests/data:/etc/loki \
	 --name vector_loki grafana/loki:master -config.file=/etc/loki/loki-config.yaml
endif

.PHONY: stop-integration-loki
stop-integration-loki:
	$(CONTAINER_TOOL) rm --force vector_loki 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-loki 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-loki 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-loki 2>/dev/null; true
endif

.PHONY: test-integration-loki
test-integration-loki: ## Runs Loki integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-loki
	$(MAKE) start-integration-loki
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features loki-integration-tests --lib ::loki:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-loki
endif

# https://docs.mongodb.com/manual/tutorial/deploy-shard-cluster/
.PHONY: start-integration-mongodb_metrics
start-integration-mongodb_metrics:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-mongodb_metrics -p 27017:27017 -p 27018:27018 -p 27019:27019
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-mongodb_metrics --name vector_mongodb_metrics1 mongo:4.2.10 mongod --configsvr --replSet vector
	sleep 1
	$(CONTAINER_TOOL) exec vector_mongodb_metrics1 mongo --port 27019 --eval 'rs.initiate({_id:"vector",configsvr:true,members:[{_id:0,host:"127.0.0.1:27019"}]})'
	$(CONTAINER_TOOL) exec -d vector_mongodb_metrics1 mongos --port 27018 --configdb vector/127.0.0.1:27019
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-mongodb_metrics --name vector_mongodb_metrics2 mongo:4.2.10 mongod
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-mongodb_metrics
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-mongodb_metrics -p 27018:27018 -p 27019:27019 --name vector_mongodb_metrics1 mongo:4.2.10 mongod --configsvr --replSet vector
	sleep 1
	$(CONTAINER_TOOL) exec vector_mongodb_metrics1 mongo --port 27019 --eval 'rs.initiate({_id:"vector",configsvr:true,members:[{_id:0,host:"127.0.0.1:27019"}]})'
	$(CONTAINER_TOOL) exec -d vector_mongodb_metrics1 mongos --port 27018 --configdb vector/127.0.0.1:27019
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-mongodb_metrics -p 27017:27017 --name vector_mongodb_metrics2 mongo:4.2.10 mongod
endif

.PHONY: stop-integration-mongodb_metrics
stop-integration-mongodb_metrics:
	$(CONTAINER_TOOL) rm --force vector_mongodb_metrics1 2>/dev/null; true
	$(CONTAINER_TOOL) rm --force vector_mongodb_metrics2 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-mongodb_metrics 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-mongodb_metrics 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-mongodb_metrics 2>/dev/null; true
endif

.PHONY: test-integration-mongodb_metrics
test-integration-mongodb_metrics: ## Runs MongoDB Metrics integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-mongodb_metrics
	$(MAKE) start-integration-mongodb_metrics
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features mongodb_metrics-integration-tests --lib ::mongodb_metrics:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-mongodb_metrics
endif

.PHONY: start-integration-nats
start-integration-nats:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-nats -p 4222:4222
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-nats  --name vector_nats \
	 nats
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-nats
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-nats -p 4222:4222 --name vector_nats \
	 nats
endif

.PHONY: stop-integration-nats
stop-integration-nats:
	$(CONTAINER_TOOL) rm --force vector_nats 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-nats 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-nats 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-nats 2>/dev/null; true
endif

.PHONY: test-integration-nats
test-integration-nats: ## Runs NATS integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-nats
	$(MAKE) start-integration-nats
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features nats-integration-tests --lib ::nats:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-nats
endif

.PHONY: start-integration-nginx
start-integration-nginx:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-nginx -p 8010:8000
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-nginx --name vector_nginx \
	-v $(PWD)tests/data/nginx/:/etc/nginx:ro nginx:1.19.4
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-nginx
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-nginx -p 8010:8000 --name vector_nginx \
	-v $(PWD)/tests/data/nginx/:/etc/nginx:ro nginx:1.19.4
endif

.PHONY: stop-integration-nginx
stop-integration-nginx:
	$(CONTAINER_TOOL) rm --force vector_nginx 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-nginx 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-nginx 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-nginx 2>/dev/null; true
endif

.PHONY: test-integration-nginx
test-integration-nginx: ## Runs nginx integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-nginx
	$(MAKE) start-integration-nginx
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features nginx-integration-tests --lib ::nginx:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-nginx
endif

.PHONY: start-integration-prometheus stop-integration-prometheus test-integration-prometheus
start-integration-prometheus:
	$(CONTAINER_TOOL) run -d --name vector_prometheus --net=host \
	 --volume $(PWD)/tests/data:/etc/vector:ro \
	 prom/prometheus --config.file=/etc/vector/prometheus.yaml

stop-integration-prometheus:
	$(CONTAINER_TOOL) rm --force vector_prometheus 2>/dev/null; true

.PHONY: test-integration-prometheus
test-integration-prometheus: ## Runs Prometheus integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-influxdb
	-$(MAKE) -k stop-integration-prometheus
	$(MAKE) start-integration-influxdb
	$(MAKE) start-integration-prometheus
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features prometheus-integration-tests --lib ::prometheus:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-influxdb
	$(MAKE) -k stop-integration-prometheus
endif

.PHONY: start-integration-pulsar
start-integration-pulsar:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-pulsar -p 6650:6650
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-pulsar  --name vector_pulsar \
	 apachepulsar/pulsar bin/pulsar standalone
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-pulsar
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-pulsar -p 6650:6650 --name vector_pulsar \
	 apachepulsar/pulsar bin/pulsar standalone
endif

.PHONY: stop-integration-pulsar
stop-integration-pulsar:
	$(CONTAINER_TOOL) rm --force vector_pulsar 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-pulsar 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-pulsar 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-pulsar 2>/dev/null; true
endif

.PHONY: test-integration-pulsar
test-integration-pulsar: ## Runs Pulsar integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-pulsar
	$(MAKE) start-integration-pulsar
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features pulsar-integration-tests --lib ::pulsar:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-pulsar
endif

.PHONY: start-integration-splunk
start-integration-splunk:
# TODO Replace  timberio/splunk-hec-test:minus_compose image with production image once merged
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-splunk -p 8088:8088 -p 8000:8000 -p 8089:8089
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-splunk \
     --name splunk timberio/splunk-hec-test:minus_compose
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-splunk
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-splunk -p 8088:8088 -p 8000:8000 -p 8089:8089 \
     --name splunk timberio/splunk-hec-test:minus_compose
endif

.PHONY: stop-integration-splunk
stop-integration-splunk:
	$(CONTAINER_TOOL) rm --force splunk 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-splunk 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-splunk 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-splunk 2>/dev/null; true
endif

.PHONY: test-integration-splunk
test-integration-splunk: ## Runs Splunk integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-splunk
	$(MAKE) start-integration-splunk
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features splunk-integration-tests --lib ::splunk_hec:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-splunk
endif

.PHONY: test-e2e-kubernetes
test-e2e-kubernetes: ## Runs Kubernetes E2E tests (Sorry, no `ENVIRONMENT=true` support)
	@scripts/test-e2e-kubernetes.sh

.PHONY: test-shutdown
test-shutdown: ## Runs shutdown tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-kafka
	$(MAKE) start-integration-kafka
	sleep 30 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features shutdown-tests --test shutdown -- --test-threads 4
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-kafka
endif

.PHONY: test-cli
test-cli: ## Runs cli tests
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --test cli -- --test-threads 4

.PHONY: build-wasm-tests
test-wasm-build-modules: $(WASM_MODULE_OUTPUTS) ### Build all WASM test modules

$(WASM_MODULE_OUTPUTS): MODULE = $(notdir $@)
$(WASM_MODULE_OUTPUTS): ### Build a specific WASM module
	@echo "# Building WASM module ${MODULE}, requires Rustc for wasm32-wasi."
	${MAYBE_ENVIRONMENT_EXEC} cargo build \
		--target-dir target/ \
		--manifest-path tests/data/wasm/${MODULE}/Cargo.toml \
		--target wasm32-wasi \
		--release \
		--package ${MODULE}

.PHONY: test-wasm
test-wasm: export TEST_THREADS=1
test-wasm: export TEST_LOG=vector=trace
test-wasm: $(WASM_MODULE_OUTPUTS)  ### Run engine tests
	${MAYBE_ENVIRONMENT_EXEC} cargo test wasm --no-fail-fast --no-default-features --features "transforms-field_filter transforms-wasm transforms-lua transforms-add_fields" --lib --all-targets -- --nocapture
