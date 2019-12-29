#encoding: utf-8

class Field
  include Comparable

  TYPES = ["*", "bool", "double", "[double]", "int", "[int]", "map", "string", "[string]", "struct", "timestamp"].freeze

  class << self
    def build_struct(hash)
      fields = OpenStruct.new()

      hash.each do |field_name, field_hash|
        field = new(
          field_hash.merge({"name" => field_name}
        ))

        fields.send("#{field_name}=", field)
      end

      fields
    end
  end

  attr_reader :name,
    :default,
    :description,
    :enum,
    :examples,
    :fields,
    :optional,
    :type

  def initialize(hash)
    @name = hash.fetch("name")
    @default = hash["default"]
    @description = hash.fetch("description")
    @enum = hash["enum"]
    @optional = hash.fetch("optional")
    @type = hash.fetch("type")

    if @type == "bool"
      @examples = [true, false]
    end

    if @type != "struct"
      @examples = @examples || hash["examples"] || @enum || raise("#{self.class.name}#examples must be an array of examples for #{@name}")
    end

    # Coercion

    if @type == "timestamp"
      @examples = @examples.collect do |example|
        DateTime.iso8601(example)
      end
    end

    # Sub-fields

    @fields = OpenStruct.new()

    (hash["fields"] || {}).each do |field_name, field_hash|
      field = Field.new(
        field_hash.merge({"name" => field_name}
      ))

      @fields.send("#{field_name}=", field)
    end

    # Validations

    if !TYPES.include?(@type)
      raise "#{self.class.name}#type must be one of #{TYPES.to_sentence} for #{@name}, you passed: #{@type}"
    end
  end

  def <=>(other)
    name <=> other.name
  end

  def eql?(other)
    self.<=>(other) == 0
  end

  def fields?
    fields_list.any?
  end

  def fields_list
    @fields_list ||= fields.to_h.values.sort
  end

  def optional?
    @optional ==  true
  end

  def required?
    !optional?
  end

  def wildcard?
    name.start_with?("`<")
  end
end
