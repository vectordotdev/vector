#encoding: utf-8

class Field
  include Comparable

  OBJECT_TYPES = ["struct", "table"]

  attr_reader :name,
    :category,
    :children,
    :default,
    :description,
    :display,
    :enum,
    :examples,
    :groups,
    :partition_key,
    :prioritize,
    :relevant_when,
    :required,
    :templateable,
    :type,
    :unit

  def initialize(hash)
    @children = (hash["children"] || {}).to_struct_with_name(self.class)
    @common = hash["common"] == true
    @default = hash["default"]
    @description = hash.fetch("description")
    @display = hash["display"]
    @enum = hash["enum"]
    @examples = hash["examples"] || []
    @groups = hash["groups"] || []
    @name = hash.fetch("name")
    @partition_key = hash["partition_key"] == true
    @prioritize = hash["prioritize"] == true
    @relevant_when = hash["relevant_when"]
    @required = hash["required"] == true
    @templateable = hash["templateable"] == true
    @type = hash.fetch("type")
    @unit = hash["unit"]

    @category = hash["category"] || ((@children.to_h.values.empty? || inline?) ? "General" : @name.humanize)

    if @required == true && !@defualt.nil?
      raise ArgumentError.new("#{self.class.name}#required must be false if there is a default for field #{@name}")
    end

    if !@relevant_when.nil? && !@relevant_when.is_a?(Hash)
      raise ArgumentError.new("#{self.class.name}#relevant_when must be a hash of conditions")
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

    # Coercion

    if @type == "timestamp"
      @examples = @examples.collect do |example|
        DateTime.iso8601(example)
      end
    end

    # Requirements

    if @examples.empty? && @children.empty? && !object? && !array_of_objects?
      raise "#{self.class.name}#examples is required if a #default is not specified for #{@name}"
    end

    if wildcard?
      if !@examples.any? { |example| example.is_a?(Hash) }
        raise "#{self.class.name}#examples must be a hash with name/value keys when the name is \"*\""
      end
    end
  end

  def <=>(other)
    if prioritize? && !other.prioritize?
      -1
    elsif !wildcard? && other.wildcard?
      -1
    else
      name <=> other.name
    end
  end

  def advanced?
    if children.any?
      children_list.any?(&:advanced?)
    else
      !common?
    end
  end

  def array_of_objects?
    OBJECT_TYPES.any? do |object_type|
      type == "[#{object_type}]"
    end
  end

  def children?
    children.any?
  end

  def children_list
    @children_list ||= @children.to_h.values.sort
  end

  def common?
    @common == true || required?
  end

  def common_children
    @common_children ||= children.select(&:common?)
  end

  def config_file_sort_token
    first =
      if object?
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

  def group?(group_name)
    groups.any? do |group|
      group.downcase == group_name.downcase
    end
  end

  def human_default
    "#{default} #{unit}"
  end

  def inline?
    display == "inline"
  end

  def object?
    OBJECT_TYPES.include?(type)
  end

  def optional?
    !required?
  end

  def partition_key?
    partition_key == true
  end

  def prioritize?
    prioritize == true
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

  def templateable?
    templateable == true
  end

  def to_h
    {
      name: name,
      category: category,
      children: children,
      default: default,
      description: description,
      display: display,
      enum: enum,
      examples: examples,
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
