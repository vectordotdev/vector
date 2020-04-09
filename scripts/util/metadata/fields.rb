require_relative "field"

class Fields
  attr_reader :fields, :global_log_schema_key

  def initialize(hash)
    @fields = (hash["fields"] || {}).to_struct_with_name(constructor: Field)
    @global_log_schema_key = hash["global_log_schema_key"]
  end

  def fields_list
    @fields_list ||= fields.to_h.values.sort
  end
end
