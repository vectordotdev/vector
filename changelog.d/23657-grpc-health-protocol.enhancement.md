`vector` source: Implement standard gRPC health checking protocol (`grpc.health.v1.Health`)
alongside the existing custom health check endpoint. This enables compatibility with standard
tools like `grpc-health-probe` for Kubernetes and other orchestration systems.

Issue: https://github.com/vectordotdev/vector/issues/23657

authors: jpds
