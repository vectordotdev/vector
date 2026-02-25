# Makefile for generating protobuf test fixtures

# Default target
all: generate-desc generate-pb

# Generates .desc descriptor sets for tests
generate-desc:
	protoc --proto_path=protos \
	       --include_imports \
	       --descriptor_set_out=protos/test_protobuf.desc \
	       protos/test_protobuf.proto

	protoc --proto_path=protos \
	       --include_imports \
	       --descriptor_set_out=protos/test_protobuf3.desc \
	       protos/test_protobuf3.proto

# Generates serialized binary .pb test files from .txt input
generate-pb:
	protoc --proto_path=protos \
	       --encode=test_protobuf.Person \
	       protos/test_protobuf.proto \
	       < pbs/person_someone.txt > pbs/person_someone.pb

	protoc --proto_path=protos \
	       --encode=test_protobuf3.Person \
	       protos/test_protobuf3.proto \
	       < pbs/person_someone3.txt > pbs/person_someone3.pb

# Clean generated files
clean:
	rm -f protos/*.desc
	rm -f pbs/*.pb

# Help target
help:
	@echo "Protobuf Test Fixture Generator"
	@echo ""
	@echo "Targets:"
	@echo "  all            - Generate both descriptor sets and binary test files (default)"
	@echo "  generate-desc  - Generate .desc descriptor set files from .proto files"
	@echo "  generate-pb    - Generate .pb binary files from .txt text format inputs"
	@echo "  clean          - Remove all generated files"
	@echo "  help           - Show this help message"
	@echo ""
	@echo "Usage:"
	@echo "  1. Edit or add .proto files in protos/"
	@echo "  2. Create human-readable .txt files in pbs/"
	@echo "  3. Run 'make' to regenerate all test fixtures"

.PHONY: all generate-desc generate-pb clean help
