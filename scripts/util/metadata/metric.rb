require_relative "field"

class Metric
  attr_reader :schema

  def initialize(hash)
    @schema = hash.fetch("schema").to_struct_with_name(constructor: Field)
  end

  def schema_list
    @schema_list ||= schema.to_h.values.sort
  end
end
