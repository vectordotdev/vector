class Option
  include Comparable

  attr_reader :name,
    :category,
    :default,
    :description,
    :enum,
    :example,
    :example_key,
    :null,
    :options,
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
    @category = hash["category"] || (@options.nil? ? "General" : @name.humanize)
    @default = hash["default"]
    @description = hash.fetch("description")
    @enum = hash["enum"]
    @example = hash["example"]
    @example_key = hash["example_key"]
    @null = hash["null"].nil? ? true : hash["null"] == true
    @type = hash.fetch("type")
    @unit = hash["unit"]

    if !@default.nil? && !@example.nil?
      raise "#{self.class.name}#example not required when a #default is specified for #{@name}"
    end

    if !@enum.nil? && !@example.nil?
      raise "#{self.class.name}#example not required when a #enum is specified for #{@name}"
    end

    if @example.nil?
      if !@default.nil?
        @example = @default
      elsif !@enum.nil?
        @example = @enum.first
      end
    end

    if @example.nil? && @options.nil?
      raise "#{self.class.name}#example is required if a #default is not specified"
    end

    if @example_key.nil? && @name == "*"
      raise "#{self.class.name}#example_key is required if the name is `*`"
    end
  end

  def <=>(other_option)
    sort_token <=> other_option.sort_token
  end

  def get_relevant_sections(sections)
    sections.select do |section|
      section.referenced_options.include?(name)
    end
  end

  def optional?
    !required?
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
end