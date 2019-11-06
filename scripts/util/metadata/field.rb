#encoding: utf-8

class Field
  include Comparable

  TYPES = ["bool", "float", "int", "string", "timestamp"]

  attr_reader :name,
    :description,
    :fields,
    :type

  def initialize(hash)
    @name = hash.fetch("name")
    @description = hash.fetch("description")
    @type = hash.fetch("type")

    # Sub-fields

    @fields = OpenStruct.new()

    (hash["fields"] || {}).each do |field_name, field_hash|
      field = Field.new(
        field_hash.merge({"name" => field_name}
      ))

      @fields.send("#{field_name}=", field)
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
end