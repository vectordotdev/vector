variable "region" {
  type    = string
  default = "us-east-1"
}

variable "cluster_suffix" {
  type        = string
  description = "Optional suffix appended to cluster resource names (e.g. your username); set this if the default \"vector-perf\" name might collide with another concurrent reproduction in the same AWS account"
  default     = ""
}

variable "ami_id" {
  type        = string
  description = "AMI to use for the K3s node. Defaults to the Ubuntu 22.04 (Jammy) AMI this guide's results were measured against; override if you change `region` or want a newer image."
  default     = "ami-0d28727121d5d4a3c" # ubuntu/images/hvm-ssd/ubuntu-jammy-22.04-amd64-server-20260702, us-east-1
}

variable "node_instance_type" {
  type = string
  # c5.4xlarge: 16 vCPU, 32 GiB
  # Phase 3 (8-worker) CPU requests: 8×1000m Vector + 5×100m producers + 500m consumer + ~200m system ≈ 9.2 vCPU
  default = "c5.4xlarge"
}

variable "my_cidr" {
  type        = string
  description = "CIDR to allow SSH access to the K3s instance (e.g. 1.2.3.4/32)"
}

variable "ssh_public_key_path" {
  type    = string
  default = "~/.ssh/vector_tests.pub"
}

variable "ssh_private_key_path" {
  type    = string
  default = "~/.ssh/vector_tests"
}
