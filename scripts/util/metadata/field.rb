#encoding: utf-8

class Field
  # This classes was introduced to handle value grouping without breaking
  # the current Array API.
  #
  # We introduced "groups" for component options. For example, the
  # `influxdb_metrics` sink groups options by "v1" and "v2". This signals
  # which options are supported across each InfluxDB version. The problem
  # is that some of the option values differ based on the version being used.
  # This class allows definitions to group example values in a backwards
  # compatible way.
  class Examples < Array
    def initialize(examples)
      groups = {}
      array = []

      if examples.is_a?(Hash)
        groups = examples

        examples.values.each do |value|
          array += value
        end
      elsif examples.is_a?(Array)
        groups = {"all" => examples}
        array = examples
      else
        raise ArgumentError.new("Unsupported examples type: #{examples.class.name}")
      end

      @groups = groups

      super(array)
    end

    def fetch_group_values!(group)
      @groups.key?(group) ? @groups.fetch(group) : self
    end

    def inspect
      "Fields::Examples<groups=#{@groups.inspect} array=#{super}>"
    end
  end

  include Comparable

  OBJECT_TYPES = ["struct", "table"]

  attr_reader :name,
    :category,
    :children,
    :default,
    :default_label,
    :description,
    :enum,
    :examples,
    :field_path_notation,
    :groups,
    :partition_key,
    :relevant_when,
    :required,
    :sort,
    :templateable,
    :toml_display,
    :type,
    :unit,
    :warnings

  def initialize(hash)
    @children = (hash["children"] || {}).to_struct_with_name(constructor: self.class)
    @common = hash["common"]
    @default = hash["default"]
    @description = hash.fetch("description")
    @enum = hash["enum"]
    @examples = Examples.new(hash["examples"] || []).freeze
    @field_path_notation = hash["field_path_notation"] == true
    @groups = (hash["groups"] || []).freeze
    @name = hash.fetch("name")
    @partition_key = hash["partition_key"] == true
    @relevant_when = hash["relevant_when"]
    @required = hash["required"] == true
    @sort = hash["sort"]
    @templateable = hash["templateable"] == true
    @toml_display = hash["toml_display"]
    @type = hash.fetch("type")
    @unit = hash["unit"]
    @warnings = (hash["warnings"] || []).collect(&:to_struct).freeze

    # category

    @category = hash["category"] || ((@children.to_h.values.empty?) ? "General" : @name.humanize)

    # Requirements

    if @name.include?("`<")
      raise ArgumentError.new("#{@name}.name must be in the format of \"`[..]`\" instead of \"<...>\"")
    end

    if @required == true && !@default.nil?
      raise ArgumentError.new("#{@name}.required must be false if there is a default")
    end

    if wildcard? && !object?
      if !@examples.any? { |example| example.is_a?(Hash) }
        raise ArgumentError.new("#{@name}#examples must be a hash with name/value keys when the name is \"*\"")
      end
    end

    if @examples.any? && !@enum.nil? && !wildcard? && @examples != @enum.keys
      raise ArgumentError.new("#{@name}.examples is invalid, remove it or match it exactly with the enum values")
    end

    if !@relevant_when.nil? && !@relevant_when.is_a?(Hash)
      raise ArgumentError.new("#{@name}.relevant_when must be a hash of conditions")
    end

    # Examples

    if @examples.empty?
      if !@enum.nil?
        @examples = Examples.new(@enum.keys)
      elsif !@default.nil?
        @examples = Examples.new([@default])
        if @type == "bool"
          @examples.push(!@default)
        end
      elsif @type == "bool"
        @examples = Examples.new([true, false])
      end
    end

    # Coercion

    if @type == "timestamp"
      @examples =
        Examples.new(
          @examples.collect do |example|
            DateTime.iso8601(example)
          end
        )
    end

    # Requirements

    if @examples.empty? && !wildcard? && @children.empty? && !object? && !array_of_objects?
      raise "#{@name}#examples is required if a #default is not specified"
    end
  end

  def <=>(other)
    if !wildcard? && other.wildcard?
      -1
    else
      name.downcase <=> other.name.downcase
    end
  end

  def advanced?
    if children.any?
      children_list.any?(&:advanced?)
    else
      !common?
    end
  end

  def all_warnings
    @all_warnings ||=
      begin
        new_warnings = []

        new_warnings +=
          warnings.collect do |warning|
            warning["option_name"] = name
            warning
          end

        new_warnings +=
          children_list.collect do |child|
            child.all_warnings
          end.
          flatten

        new_warnings.freeze
      end
  end

  def array?
    type.start_with?("[")
  end

  def array_of_objects?
    OBJECT_TYPES.any? do |object_type|
      type == "[#{object_type}]"
    end
  end

  def config_sort_obj
    @config_sort_obj ||= [sort || 99, "#{category}#{name}".downcase]
  end

  def object_of_object?
    object? && children_list.length == 1 && children_list[0].object?
  end

  def children?
    children.any?
  end

  def children_list
    @children_list ||= @children.to_h.values.sort
  end

  def common?
    @common == true || (@common.nil? && required?)
  end

  def common_children
    @common_children ||= children.select(&:common?)
  end

  def context?
    category.downcase == "context"
  end

  def default_infinity?
    default == 18446744073709551615
  end

  def eql?(other)
    self.<=>(other) == 0
  end

  def field_path_notation?
    @field_path_notation == true
  end

  def get_relevant_sections(sections)
    sections.select do |section|
      section.referenced_options.include?(name) ||
        section.referenced_options.any? { |o| o.end_with?(name) }
    end
  end

  def group?(group_name)
    if group_name.nil?
      true
    else
      groups.any? do |group|
        group.downcase == group_name.downcase
      end
    end
  end

  def human_default
    "#{default} #{unit}"
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

  def relevant_when?
    !relevant_when.nil?
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

  def sort?
    @sort != nil
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
