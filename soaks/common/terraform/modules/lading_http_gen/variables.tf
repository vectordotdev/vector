variable "type" {
  description = "The type of the vector install, whether 'baseline' or 'comparison'"
  type        = string
}

variable "namespace" {
  description = "The namespace in which to run"
  type        = string
}

variable "http-gen-yaml" {
  description = "The rendered http_gen.yaml for this test"
  type        = string
}

variable "http-gen-static-bootstrap" {
  description = "Boostrap log to be used for static variant, mounted at /data/boostrap.log"
  type        = string
  default     = ""
}


variable "lading_image" {
  description = "The lading image to run"
  type        = string
}
