variable "region" {
  type    = string
  default = "us-east-1"
}

variable "user_id" {
  type    = string
  default = "thomas"
}

variable "subnet_ids" {
  type = list(string)
  # Default VPC subnets in us-east-1 — two AZs for node group HA requirement
  default = [
    "subnet-037e09807704014c3", # us-east-1a
    "subnet-054faaea95ca6c566", # us-east-1c
  ]
}

variable "node_instance_type" {
  type    = string
  # c5.4xlarge: 16 vCPU, 32 GiB
  # Phase 3 (8-worker) CPU requests: 8×1000m Vector + 5×100m producers + 500m consumer + ~200m system ≈ 9.2 vCPU
  default = "c5.4xlarge"
}

variable "node_count" {
  type    = number
  default = 2
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
