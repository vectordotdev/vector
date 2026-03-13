# Full docs build with all dependencies for local development
docs:
	../scripts/environment/prepare.sh --modules=protoc
	cargo doc --no-default-features --features="docs" --workspace
# rust-doc.vector.dev specific build without the extra dependencies
ci-docs-build:
	../scripts/environment/prepare.sh --modules=protoc
	cargo doc --no-default-features --features="docs" --no-deps --workspace
	cargo doc --no-deps -p vrl
