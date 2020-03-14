set -eu

# Install kubectl
curl -LO https://storage.googleapis.com/kubernetes-release/release/v1.17.0/bin/linux/amd64/kubectl 
chmod +x ./kubectl
mv ./kubectl /usr/local/bin/kubectl

# Install minikube
curl -Lo minikube https://storage.googleapis.com/minikube/releases/latest/minikube-linux-amd64 
chmod +x minikube

# Install docker cli
apt-get install docker.io -y 

# Start image registry
(docker run -d -p 5324:5000 --restart=always registry:2 || true)

# Build & push test image
docker build -t "localhost:5324/vector_test:latest" -f - . << EOF
FROM buildpack-deps:18.04-curl
COPY ./target/x86_64-unknown-linux-musl/debug/vector /usr/local/bin
RUN chmod +x /usr/local/bin/vector
ENTRYPOINT ["/usr/local/bin/vector"]
EOF
docker push localhost:5324/vector_test:latest       

# Test Kubernetes v1.17.2
(./minikube start --kubernetes-version=v1.17.2 || CHANGE_MINIKUBE_NONE_USER=true ./minikube start --vm-driver=none --kubernetes-version=v1.17.2)
KUBE_TEST_IMAGE=localhost:5324/vector_test:latest cargo test --lib --features "sources-kubernetes transforms-kubernetes kubernetes-integration-tests" -- --test-threads=1 kubernetes
./minikube stop

# # Test Kubernetes v1.14.10
# CHANGE_MINIKUBE_NONE_USER=true ./minikube start --vm-driver=none --kubernetes-version=v1.14.10
# KUBE_TEST_IMAGE=localhost:5324/vector_test:latest cargo test --lib --features "sources-kubernetes transforms-kubernetes kubernetes-integration-tests" -- --test-threads=1 kubernetes
# ./minikube stop

# # Test Kubernetes v1.13.2
# CHANGE_MINIKUBE_NONE_USER=true ./minikube start --vm-driver=none --kubernetes-version=v1.13.2
# KUBE_TEST_IMAGE=localhost:5324/vector_test:latest cargo test --lib --features "sources-kubernetes transforms-kubernetes kubernetes-integration-tests" -- --test-threads=1 kubernetes
# ./minikube stop 