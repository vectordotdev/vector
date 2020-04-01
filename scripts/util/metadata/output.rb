require_relative "example"
require_relative "field"

class Output
  attr_reader :examples, :fields

  def initialize(hash)
    @examples = (hash["examples"] || []).collect { |e| Example.new(e) }
    @fields = (hash["fields"] || {}).to_struct_with_name(constructor: Field)
  end
end
