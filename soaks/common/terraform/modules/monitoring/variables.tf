variable "experiment_name" {
  description = "The name of the experiment"
  type        = string
}

variable "experiment_target" {
  description = "The target platform of the experiment"
  type        = string
}

variable "variant" {
  description = "The variant of the vector experiment, whether 'baseline' or 'comparison'"
  type        = string
}

variable "vector_image" {
  description = "The image of vector to use in this investigation"
  type        = string
}
