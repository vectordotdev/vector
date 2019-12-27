require_relative "field"

class Output
  attr_reader :examples, :fields

  def initialize(hash)
    @examples = (hash["examples"] || []).collect { |e| OpenStruct.new(e) }
    @fields = Field.build_struct(hash["fields"] || {})
  end
end