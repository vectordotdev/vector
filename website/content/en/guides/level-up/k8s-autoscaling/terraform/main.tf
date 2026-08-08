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
  cluster_name = var.cluster_suffix != "" ? "vector-perf-${var.cluster_suffix}" : "vector-perf"
  tags = {
    Project   = "vector-perf"
    ManagedBy = "terraform"
  }
}

# ── Key pair ───────────────────────────────────────────────────────────────────

resource "aws_key_pair" "this" {
  key_name   = local.cluster_name
  public_key = file(pathexpand(var.ssh_public_key_path))
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

# ── EC2 instance ───────────────────────────────────────────────────────────────

resource "aws_instance" "k3s" {
  ami                         = var.ami_id
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
    set -eo pipefail

    # Install K3s — include the public IP in the TLS SAN so kubectl works directly
    # Poll IMDS until the public IP is available (avoids a race on first boot).
    # --retry tolerates transient IMDS timeouts/hiccups instead of aborting
    # the whole script under `set -e`.
    IMDS_TOKEN=$(curl -sf --max-time 3 --retry 5 --retry-delay 2 --retry-connrefused \
      -X PUT "http://169.254.169.254/latest/api/token" \
      -H "X-aws-ec2-metadata-token-ttl-seconds: 60")
    until PUBLIC_IP=$(curl -sf --max-time 3 -H "X-aws-ec2-metadata-token: $IMDS_TOKEN" \
        http://169.254.169.254/latest/meta-data/public-ipv4) && [ -n "$PUBLIC_IP" ]; do
      sleep 2
      IMDS_TOKEN=$(curl -sf --max-time 3 --retry 5 --retry-delay 2 --retry-connrefused \
        -X PUT "http://169.254.169.254/latest/api/token" \
        -H "X-aws-ec2-metadata-token-ttl-seconds: 60")
    done
    # Write config.yaml so the SAN persists across cert regenerations
    mkdir -p /etc/rancher/k3s
    printf 'tls-san:\n  - %s\n' "$PUBLIC_IP" > /etc/rancher/k3s/config.yaml
    curl -sfL https://get.k3s.io | INSTALL_K3S_VERSION="v1.36.2+k3s1" INSTALL_K3S_EXEC="--disable=traefik,servicelb --tls-san $PUBLIC_IP" sh -

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
    interpreter = ["/bin/bash", "-c"]
    command     = <<-CMD
      set -eo pipefail
      deadline=$(( $(date +%s) + 300 ))
      until ssh -i ${var.ssh_private_key_path} -o StrictHostKeyChecking=no -o ConnectTimeout=5 \
          ubuntu@${aws_instance.k3s.public_ip} 'systemctl is-active k3s' 2>/dev/null; do
        if [ "$(date +%s)" -ge "$deadline" ]; then
          echo "ERROR: K3s did not become active within 300 s. Dumping cloud-init log:" >&2
          ssh -i ${var.ssh_private_key_path} -o StrictHostKeyChecking=no \
            ubuntu@${aws_instance.k3s.public_ip} \
            'sudo journalctl -u k3s --no-pager -n 50 2>/dev/null || sudo tail -50 /var/log/cloud-init-output.log' \
            >&2 || true
          exit 1
        fi
        sleep 5
      done
      ssh -i ${var.ssh_private_key_path} -o StrictHostKeyChecking=no \
          ubuntu@${aws_instance.k3s.public_ip} 'sudo cat /etc/rancher/k3s/k3s.yaml' \
        | sed 's|https://127.0.0.1|https://${aws_instance.k3s.public_ip}|g' \
        > ${path.module}/kubeconfig.tmp
      if [ ! -s ${path.module}/kubeconfig.tmp ]; then
        echo "ERROR: fetched kubeconfig is empty" >&2
        rm -f ${path.module}/kubeconfig.tmp
        exit 1
      fi
      mv ${path.module}/kubeconfig.tmp ${path.module}/kubeconfig
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
