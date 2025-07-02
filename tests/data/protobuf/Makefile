generate-desc:
	protoc -I=. -o test_proto.desc test_proto.proto

generate-pb2:
	@protoc --python_out=. test_proto.proto

check-protobuf:
	@python3 -c "import google.protobuf" 2>/dev/null || (echo 'protobuf is NOT installed in python3 environment' && exit 1)

generate-test-payload: check-protobuf generate-pb2
	@python3 serialize.py

