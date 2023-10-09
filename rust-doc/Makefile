# Full docs build with all dependencies for local development
docs:
	../scripts/environment/install-protoc.sh ${HOME}/protoc
	PATH=${PATH}:${HOME}/protoc/ cargo doc --no-default-features --features="docs"
# rust-doc.vector.dev specific build without the extra dependencies
ci-docs-build:
	../scripts/environment/install-protoc.sh ${HOME}/protoc
	PATH=${PATH}:${HOME}/protoc/ cargo doc --no-default-features --features="docs" --no-deps --workspace
