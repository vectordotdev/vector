variable "type" {
  description = "The type of the vector install, whether 'baseline' or 'comparison'"
  type        = string
}

variable "namespace" {
  description = "The namespace in which to run"
  type        = string
}

variable "tcp-gen-toml" {
  description = "The rendered tcp_gen.toml for this test"
  type        = string
}
