terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
    null = {
      source  = "hashicorp/null"
      version = "~> 3.0"
    }
  }
}

provider "aws" {
  region = var.region
}

locals {
  cluster_name = "vector-perf-${var.user_id}"
  tags = {
    Project   = "vector-perf"
    ManagedBy = "terraform"
    User      = var.user_id
  }
}

# ── Key pair ───────────────────────────────────────────────────────────────────

resource "aws_key_pair" "this" {
  key_name   = local.cluster_name
  public_key = file(var.ssh_public_key_path)
  tags       = local.tags
}

# ── Security group ─────────────────────────────────────────────────────────────

data "aws_vpc" "default" {
  default = true
}

resource "aws_security_group" "k3s" {
  name        = "${local.cluster_name}-k3s"
  description = "K3s single-node cluster for vector-perf benchmark"
  vpc_id      = data.aws_vpc.default.id

  # SSH access from the operator's IP only
  ingress {
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = [var.my_cidr]
  }

  # K3s API server from the operator's IP
  ingress {
    from_port   = 6443
    to_port     = 6443
    protocol    = "tcp"
    cidr_blocks = [var.my_cidr]
  }

  # Allow all traffic within the security group (pod-to-pod, K3s internals)
  ingress {
    from_port = 0
    to_port   = 0
    protocol  = "-1"
    self      = true
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = local.tags
}

# ── Lookup latest Ubuntu 22.04 AMI ────────────────────────────────────────────

data "aws_ami" "ubuntu" {
  most_recent = true
  owners      = ["099720109477"] # Canonical

  filter {
    name   = "name"
    values = ["ubuntu/images/hvm-ssd/ubuntu-jammy-22.04-amd64-server-*"]
  }

  filter {
    name   = "virtualization-type"
    values = ["hvm"]
  }
}

# ── EC2 instance ───────────────────────────────────────────────────────────────

resource "aws_instance" "k3s" {
  ami                         = data.aws_ami.ubuntu.id
  instance_type               = var.node_instance_type
  key_name                    = aws_key_pair.this.key_name
  vpc_security_group_ids      = [aws_security_group.k3s.id]
  associate_public_ip_address = true

  root_block_device {
    volume_size = 50
    volume_type = "gp3"
  }

  user_data                   = <<-USERDATA
    #!/bin/bash
    set -e

    # Install K3s — include the public IP in the TLS SAN so kubectl works directly
    # Poll IMDS until the public IP is available (avoids a race on first boot)
    until PUBLIC_IP=$(curl -s --max-time 3 http://169.254.169.254/latest/meta-data/public-ipv4) && [ -n "$PUBLIC_IP" ]; do
      sleep 2
    done
    # Write config.yaml so the SAN persists across cert regenerations
    mkdir -p /etc/rancher/k3s
    printf 'tls-san:\n  - %s\n' "$PUBLIC_IP" > /etc/rancher/k3s/config.yaml
    curl -sfL https://get.k3s.io | INSTALL_K3S_EXEC="--disable=traefik,servicelb --tls-san $PUBLIC_IP" sh -

    # Make kubeconfig world-readable so ubuntu user can read it
    chmod 644 /etc/rancher/k3s/k3s.yaml

    # Install helm
    curl -fsSL https://raw.githubusercontent.com/helm/helm/main/scripts/get-helm-3 | bash

    # Install grpcurl (for scraping Vector's gRPC observability API)
    curl -sSL https://github.com/fullstorydev/grpcurl/releases/download/v1.9.3/grpcurl_1.9.3_linux_amd64.deb \
      -o /tmp/grpcurl.deb && dpkg -i /tmp/grpcurl.deb && rm /tmp/grpcurl.deb
  USERDATA
  user_data_replace_on_change = true

  tags = merge(local.tags, {
    Name = local.cluster_name
  })
}

# ── Kubeconfig ────────────────────────────────────────────────────────────────

resource "null_resource" "kubeconfig" {
  triggers = {
    instance_ip = aws_instance.k3s.public_ip
  }

  provisioner "local-exec" {
    command = <<-CMD
      until ssh -i ${var.ssh_private_key_path} -o StrictHostKeyChecking=no -o ConnectTimeout=5 \
          ubuntu@${aws_instance.k3s.public_ip} 'systemctl is-active k3s' 2>/dev/null; do
        sleep 5
      done
      ssh -i ${var.ssh_private_key_path} -o StrictHostKeyChecking=no \
          ubuntu@${aws_instance.k3s.public_ip} 'sudo cat /etc/rancher/k3s/k3s.yaml' \
        | sed 's|https://127.0.0.1|https://${aws_instance.k3s.public_ip}|g' \
        > ${path.module}/kubeconfig
      chmod 600 ${path.module}/kubeconfig
    CMD
  }

  depends_on = [aws_instance.k3s]
}

# ── Outputs ────────────────────────────────────────────────────────────────────

output "cluster_name" {
  value = local.cluster_name
}

output "instance_ip" {
  value = aws_instance.k3s.public_ip
}

output "kubeconfig_path" {
  value = "${path.module}/kubeconfig"
}
