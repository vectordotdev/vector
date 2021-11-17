variable "type" {
  description = "The type of the vector install, whether 'baseline' or 'comparison'"
  type        = string
}

variable "namespace" {
  description = "The namespace in which to run"
  type        = string
}

variable "splunk-hec-blackhole-toml" {
  description = "The rendered splunk_hec_blackhole.toml for this test"
  type        = string
}
