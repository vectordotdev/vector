FROM vector_build-x86_64:latest

WORKDIR /vector

# Install Kubernetes kind
ADD https://github.com/kubernetes-sigs/kind/releases/download/v0.7.0/kind-linux-amd64 /usr/local/bin/kind
RUN chmod +x /usr/local/bin/kind

# Install docker cli
RUN apt-get install docker.io -y

# Install other utility
RUN apt-get install apt-utils
RUN apt-get install libssl-dev
RUN apt-get install pkg-config
