#encoding: utf-8

require_relative "guide"
require_relative "option"
require_relative "section"

class Component
  include Comparable

  attr_reader :beta,
    :guides,
    :name,
    :correctness_tests,
    :diagram,
    :options,
    :performance_tests,
    :resources,
    :sections

  attr_accessor :alternatives

  def initialize(hash)
    #
    # Base attributes
    #

    @alternatives = []
    @beta = hash["beta"] == true
    @name = hash.fetch("name")
    @type = type
    @diagram = "#{ASSETS_PATH}#{@name}-#{@type}.svg"
    @options = OpenStruct.new()

    (hash["options"] || {}).each do |option_name, option_hash|
      option = Option.new(
        option_hash.merge({"name" => option_name}
      ))

      @options.send("#{option_name}=", option)
    end

    @resources = (hash["resources"] || []).collect do |resource_hash|
      OpenStruct.new(resource_hash)
    end

    @sections = (hash["sections"] || []).collect do |section_hash|
      Section.new(section_hash)
    end

    #
    # Tests
    #
    # Select tests based on the component name. Tests commonly include
    # the component name.
    #

    test_keyword = @name.gsub(/_(decoder|parser)$/, "")

    @correctness_tests = CORRECTNESS_TESTS.select do |test|
      test.include?(test_keyword)
    end

    @performance_tests = PERFORMANCE_TESTS.select do |test|
      test.include?(test_keyword)
    end

    #
    # Correctness Section
    #
    # Based on the selected correctness tests, add a section showcasing this.
    #

    if @correctness_tests.length > 0
      test_list = ""

      @correctness_tests.each do |test|
        test_list << "* [`#{test}`][url.#{test}_test]\n"
      end

      test_list.strip!

      description = <<~EOF
        The `#{name}` source has been involved in the following correctness tests:

        #{test_list}

        Learn more in the [Correctness][docs.correctness] sections.
        EOF

      description.strip!

      correctness_section = @sections.find { |section| section.title == "Correctness" }

      if correctness_section
        correctness_section.description << "#### Tests\n\n#{description}"
      else
        @sections << Section.new({
          "title" => "Correctness",
          "body" => description
        })
      end
    end

    #
    # Performance Section
    #
    # Based on the selected performance tests, add a section showcasing this.
    #

    if @performance_tests.length > 0
      test_list = ""

      @performance_tests.each do |test|
        test_list << "* [`#{test}`][url.#{test}_test]\n"
      end

      test_list.strip!

      description = <<~EOF
        The `#{name}` source has been involved in the following performance tests:

        #{test_list}

        Learn more in the [Performance][docs.performance] sections.
        EOF

      description.strip!

      performance_section = @sections.find { |section| section.title == "Performance" }

      if performance_section
        performance_section.description << "#### Tests\n\n#{description}"
      else
        @sections << Section.new({
          "title" => "Performance",
          "body" => description
        })
      end
    end

    #
    # Delivery Guarantee Section
    #
    # Based on the presence of a delivery guarantee, add a section showcasing
    # this
    #

    if delivery_guarantee = hash["delivery_guarantee"]
      body =
        case delivery_guarantee
        when "at_least_once"
          <<~EOF
          This component offers an **at least once** delivery guarantee if your
          [pipeline is configured to achieve this][docs.at_least_once_delivery].
          EOF
        when "best_effort"
          <<~EOF
          Due to the nature of this component, it offers a **best effort**
          delivery guarantee.
          EOF
        else
          raise("Unknown delievery_guarantee: #{delivery_guarantee.inspect} for #{type} - #{name}")
        end

      @sections << Section.new({
          "title" => "Delivery Guarantee",
          "body" => body
        })
    end

    #
    # Type Coercion Section
    #
    # Based on the presence of a "types" option, add a type coercion section
    # explaining how to use this
    #

    if options.types && options.types.type == "table"
      body =
        <<~EOF
        You can coerce your extract values into types via the `types` table
        as shown in the examples above. The supported types are:

        | Type | Desription |
        | :--- | :--------- |
        | `string` | Coerces to a string. Generally not necessary since values are extracted as strings. |
        | `int` | Coerce to a 64 bit integer. |
        | `float` | Coerce to 64 bit floats. |
        | `bool`  | Coerces to a `true`/`false` boolean. The `1`/`0` and `t`/`f` values are also coerced. |
        EOF

      @sections << Section.new({
        "title" => "Type Coercion",
        "body" => "body"
      })
    end

    #
    # Default options
    #
    # Add default options based on the presence of attributes
    #

    @options.type = Option.new({
        "name" => "type",
        "description" => "The component type",
        "enum" => [name],
        "null" => false,
        "type" => "string"
      })

    if type != "source"
      @options.inputs = Option.new({
        "name" => "inputs",
        "description" => "A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.",
        "examples" => [["my-source-id"]],
        "null" => false,
        "type" => "[string]"
      })
    end

    #
    # Cleanup
    #

    @sections = @sections.sort_by(&:title)

    section_titles = @sections.collect(&:title)

    if section_titles != section_titles.uniq
      raise ("#{self.class.name} has duplicate sections!")
    end
  end

  def <=>(other)
    name <=> other.name
  end

  def beta?
    beta == true
  end

  def type
    self.class.name.downcase
  end
end