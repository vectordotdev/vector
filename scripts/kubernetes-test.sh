set -eu

## Cleanup setup
delete_cluster() {
  (kind delete cluster --name ${KIND_CLUSTER_NAME} || true)
}
trap delete_cluster EXIT

## Actual purpose

# desired cluster name
KIND_CLUSTER_NAME="vector_test_cluster"

# Create registry container unless it already exists
reg_name='vector-kind-registry'
running="$(docker inspect -f '{{.State.Running}}' "${reg_name}" 2>/dev/null || true)"
if [ "${running}" != 'true' ]; then
  docker run \
    -d --restart=always -p "5000:5000" --name "${reg_name}" \
    registry:2
fi
reg_ip="$(docker inspect -f '{{.NetworkSettings.IPAddress}}' "${reg_name}")"

# Build and push image
echo "Build & push test image"
cargo build --no-default-features --features default-musl --target x86_64-unknown-linux-musl
strip ./target/x86_64-unknown-linux-musl/debug/vector
docker build -t "localhost:5000/vector-test:ts" -f - . << EOF
FROM buildpack-deps:18.04-curl
COPY ./target/x86_64-unknown-linux-musl/debug/vector /usr/local/bin
ENTRYPOINT ["/usr/local/bin/vector"]
EOF
docker push localhost:5000/vector-test:ts

# Creates a cluster with the local registry enabled in containerd and runs the tests

# Eagerly try to delete cluster
delete_cluster

run_test(){
  KUBE_IMAGE=$1

  echo Testing ${KUBE_IMAGE}

  cat <<EOF | kind create cluster --name "${KIND_CLUSTER_NAME}" --image ${KUBE_IMAGE} --config=-
kind: Cluster
apiVersion: kind.x-k8s.io/v1alpha4
containerdConfigPatches:
- |-
  [plugins."io.containerd.grpc.v1.cri".registry.mirrors."localhost:5000"]
    endpoint = ["http://${reg_ip}:5000"]
EOF

  # Test Kubernetes
  KUBE_TEST_IMAGE=localhost:5000/vector-test:ts cargo test --lib --no-default-features --features "sources-kubernetes transforms-kubernetes kubernetes-integration-tests" -- --test-threads=1 kubernetes

  delete_cluster

}

# Test Kubernetes v1.17.0
run_test kindest/node:v1.17.0@sha256:9512edae126da271b66b990b6fff768fbb7cd786c7d39e86bdf55906352fdf62


# Disabled for Kind because it can be slow at randome times which trigger test timeouts.

# # Test Kubernetes v1.14.10
# run_test kindest/node:v1.14.10@sha256:81ae5a3237c779efc4dda43cc81c696f88a194abcc4f8fa34f86cf674aa14977

# # Test Kubernetes v1.13.12
# run_test kindest/node:v1.13.12@sha256:5e8ae1a4e39f3d151d420ef912e18368745a2ede6d20ea87506920cd947a7e3a
