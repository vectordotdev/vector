#encoding: utf-8

class Option
  include Comparable

  TYPES = ["*", "bool", "float", "[float]", "int", "string", "[string]", "table", "[table]"]

  attr_reader :name,
    :category,
    :default,
    :description,
    :display,
    :enum,
    :examples,
    :null,
    :options,
    :partition_key,
    :relevant_when,
    :templateable,
    :type,
    :unit

  def initialize(hash)
    # Options can have sub-options (tables)
    options_hashes = hash["options"]

    if !options_hashes.nil?
      @options =
        options_hashes.collect do |sub_name, sub_hash|
          self.class.new(sub_hash.merge("name" => sub_name))
        end
    end

    @name = hash.fetch("name")
    @default = hash["default"]
    @display = hash["display"]
    @description = hash.fetch("description")
    @enum = hash["enum"]
    @examples = hash["examples"] || []
    @null = hash.fetch("null")
    @partition_key = hash["partition_key"] == true
    @relevant_when = hash["relevant_when"]
    @templateable = hash["templateable"] == true
    @type = hash.fetch("type")
    @unit = hash["unit"]

    @category = hash["category"] || ((@options.nil? || inline?) ? "General" : @name.humanize)

    if !@null.is_a?(TrueClass) && !@null.is_a?(FalseClass)
      raise ArgumentError.new("#{self.class.name}#null must be a boolean")
    end

    if !@relevant_when.nil? && !@relevant_when.is_a?(Hash)
      raise ArgumentError.new("#{self.class.name}#null must be a hash of conditions")
    end

    if !TYPES.include?(@type)
      raise "#{self.class.name}#type must be one of #{TYPES.to_sentence} for #{@name}, you passed: #{@type}"
    end

    if @examples.empty?
      if !@enum.nil?
        @examples = @enum
      elsif !@default.nil?
        @examples = [@default]
      end
    end

    if @examples.empty? && @options.nil? && !table?
      raise "#{self.class.name}#examples is required if a #default is not specified for #{@name}"
    end

    if wildcard?
      if !@examples.any? { |example| example.is_a?(Hash) }
        raise "#{self.class.name}#examples must be a hash with name/value keys when the name is \"*\""
      end
    end
  end

  def <=>(other_option)
    sort_token <=> other_option.sort_token
  end

  def array?(inner_type)
    type == "[#{inner_type}]"
  end

  def context?
    category.downcase == "context"
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

  def partition_key?
    partition_key == true
  end

  def required?
    default.nil? && null == false
  end

  def sort_token
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
      when "type"
        "AAA #{name}"
      else
        name
      end

    [first, second, third]
  end

  def table?
    type == "table"
  end

  def templateable?
    templateable == true
  end

  def wildcard?
    name == "*"
  end
end