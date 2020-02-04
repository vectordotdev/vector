#encoding: utf-8

class Option
  include Comparable

  TYPES = ["*", "bool", "float", "[float]", "int", "string", "[string]", "table", "[table]"].freeze

  class << self
    def build_struct(hash)
      options = OpenStruct.new()

      hash.each do |option_name, option_hash|
        option = new(
          option_hash.merge({"name" => option_name}
        ))

        options.send("#{option_name}=", option)
      end

      options
    end
  end

  attr_reader :name,
    :category,
    :default,
    :description,
    :display,
    :enum,
    :examples,
    :options,
    :partition_key,
    :relevant_when,
    :required,
    :templateable,
    :type,
    :unit

  def initialize(hash)
    @common = hash["common"] == true
    @default = hash["default"]
    @description = hash.fetch("description")
    @display = hash["display"]
    @enum = hash["enum"]
    @examples = hash["examples"] || []
    @name = hash.fetch("name")
    @options = self.class.build_struct(hash["options"] || {})
    @partition_key = hash["partition_key"] == true
    @relevant_when = hash["relevant_when"]
    @required = hash["required"] == true
    @templateable = hash["templateable"] == true
    @type = hash.fetch("type")
    @unit = hash["unit"]

    @category = hash["category"] || ((@options.to_h.values.empty? || inline?) ? "General" : @name.humanize)

    if @required == true && !@defualt.nil?
      raise ArgumentError.new("#{self.class.name}#required must be false if there is a default for field #{@name}")
    end

    if !@relevant_when.nil? && !@relevant_when.is_a?(Hash)
      raise ArgumentError.new("#{self.class.name}#relevant_when must be a hash of conditions")
    end

    if !TYPES.include?(@type)
      raise "#{self.class.name}#type must be one of #{TYPES.to_sentence} for #{@name}, you passed: #{@type}"
    end

    if @examples.empty?
      if !@enum.nil?
        @examples = @enum.keys
      elsif !@default.nil?
        @examples = [@default]
        if @type == "bool"
          @examples.push(!@default)
        end
      elsif @type == "bool"
        @examples = [true, false]
      end
    end

    if @examples.empty? && @options.empty? && !table?
      raise "#{self.class.name}#examples is required if a #default is not specified for #{@name}"
    end

    if wildcard?
      if !@examples.any? { |example| example.is_a?(Hash) }
        raise "#{self.class.name}#examples must be a hash with name/value keys when the name is \"*\""
      end
    end
  end

  def <=>(other)
    name <=> other.name
  end

  def advanced?
    if options.any?
      options.any?(&:advanced?)
    else
      !common?
    end
  end

  def array?(inner_type)
    type == "[#{inner_type}]"
  end

  def common?
    @common == true || required?
  end

  def common_options
    @common_options ||= options.select(&:common?)
  end

  def config_file_sort_token
    first =
      if table?
        2
      elsif required?
        0
      else
        1
      end

    second =
      case category
      when "General"
        "AA #{category}"
      when "Requests"
        "ZZ #{category}"
      else
        category
      end

    third =
      case name
      when "inputs"
        "AAB #{name}"
      when "strategy", "type"
        "AAA #{name}"
      else
        name
      end

    [first, second, third]
  end

  def context?
    category.downcase == "context"
  end

  def eql?(other)
    self.<=>(other) == 0
  end

  def get_relevant_sections(sections)
    sections.select do |section|
      section.referenced_options.include?(name) ||
        section.referenced_options.any? { |o| o.end_with?(name) }
    end
  end

  def human_default
    "#{default} #{unit}"
  end

  def inline?
    display == "inline"
  end

  def optional?
    !required?
  end

  def options_list
    @options_list ||= @options.to_h.values.sort
  end

  def partition_key?
    partition_key == true
  end

  def relevant_when_kvs
    relevant_when.collect do |k, v|
      if v.is_a?(Array)
        v.collect do |sub_v|
          "#{k} = #{sub_v.to_toml}"
        end
      else
        "#{k} = #{v.to_toml}"
      end
    end.flatten
  end

  def required?
    @required == true
  end

  def table?
    type == "table"
  end

  def templateable?
    templateable == true
  end

  def to_h
    {
      name: name,
      category: category,
      default: default,
      description: description,
      display: display,
      enum: enum,
      examples: examples,
      options: options,
      partition_key: partition_key,
      relevant_when: relevant_when,
      required: required?,
      templateable: templateable,
      type: type,
      unit: unit
    }
  end

  def wildcard?
    name.start_with?("`[")
  end
end
