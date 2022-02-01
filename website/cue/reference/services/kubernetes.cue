package metadata

services: kubernetes: {
	name:     "Kubernetes"
	thing:    "a \(name) cluster"
	url:      urls.kubernetes
	versions: ">= 1.19"
}
