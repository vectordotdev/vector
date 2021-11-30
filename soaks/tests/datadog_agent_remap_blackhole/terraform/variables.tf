variable "experiment_name" {
  description = "The name of the vector experiment"
  type        = string
}

variable "type" {
  description = "The type of the vector install, whether 'baseline' or 'comparison'"
  type        = string
}

variable "vector_image" {
  description = "The image of vector to use in this investigation"
  type        = string
}

variable "vector_cpus" {
  description = "The total number of CPUs to give to vector"
  type        = number
}

variable "lading_image" {
  description = "The lading image to run"
  type        = string
}
