#encoding: utf-8

require_relative "../generator"
require_relative "../config/specification_generator"

module Docs
  class ConfigSpecificationGenerator < Generator
    attr_reader :specification_generator

    def initialize(schema)
      @specification_generator = Config::SpecificationGenerator.new(schema)
    end

    def generate
      <<~EOF
      ---
      description: Full Vector config specification
      ---

      #{warning}

      # Config Specification

      Below is a full config specification. Note, this file is included with
      Vector package installs, generally located at `/etc/vector/vector.spec.yml`:

      {% code-tabs %}
      {% code-tabs-item title="/etc/vector/vector.spec.toml" %}
      ```toml
      #{specification_generator.generate}
      ```
      {% endcode-tabs-item %}
      {% endcode-tabs %}
      EOF
    end
  end
end