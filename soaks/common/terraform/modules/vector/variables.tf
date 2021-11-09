variable "type" {
  description = "The type of the vector install, whether 'baseline' or 'comparison'"
  type        = string
}

variable "vector_image" {
  description = "The image of vector to use in this investigation"
  type        = string
}

variable "test_name" {
  description = "The name of the soak test"
  type        = string
}

variable "vector-toml" {
  description = "The rendered vector.toml for this test"
  type        = string
}

variable "namespace" {
  description = "The namespace in which to run"
  type        = string
}

variable "vector_cpus" {
  description = "The total number of CPUs to give to vector"
  type        = number
}
