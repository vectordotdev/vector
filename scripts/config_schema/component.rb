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

    # Init

    @alternatives = []
    @beta = hash["beta"] == true
    @name = hash.fetch("name")
    @type = type
    @diagram = "#{ASSETS_PATH}#{@name}-#{@type}.svg"
    options_hash = hash["options"] || {}
    resource_hashes = hash["resources"] || []
    section_hashes = hash["sections"] || []

    test_keyword = @name.gsub(/_(decoder|parser)$/, "")

    @correctness_tests = CORRECTNESS_TESTS.select do |test|
      test.include?(test_keyword)
    end

    @performance_tests = PERFORMANCE_TESTS.select do |test|
      test.include?(test_keyword)
    end

    # resources

    @resources = resource_hashes.collect do |resource_hash|
      OpenStruct.new(resource_hash)
    end

    # sections

    @sections = section_hashes.collect do |section_hash|
      Section.new(section_hash)
    end

    if @correctness_tests.length > 0
      test_list = ""

      @correctness_tests.each do |test|
        test_list << "* [`#{test}`][#{test}_test]\n"
      end

      test_list.strip!

      description = <<~EOF
        The `#{name}` source has been involved in the following correctness tests:

        #{test_list}

        Learn more in the [Correctness][correctness] sections.
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

    if @performance_tests.length > 0
      test_list = ""

      @performance_tests.each do |test|
        test_list << "* [`#{test}`][#{test}_test]\n"
      end

      test_list.strip!

      description = <<~EOF
        The `#{name}` source has been involved in the following performance tests:

        #{test_list}

        Learn more in the [Performance][performance] sections.
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

    if delivery_guarantee = hash["delivery_guarantee"]
      body =
        case delivery_guarantee
        when "at_least_once"
          <<~EOF
          This component offers an **at least once** delivery guarantee if your
          [pipeline is configured to achieve this][at_least_once_delivery].
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

    @sections = @sections.sort_by(&:title)

    section_titles = @sections.collect(&:title)

    if section_titles != section_titles.uniq
      raise ("#{self.class.name} has duplicate sections!")
    end

    # options

    @options = OpenStruct.new()

    options_hash.each do |option_name, option_hash|
      option = Option.new(
        option_hash.merge({"name" => option_name}
      ))

      @options.send("#{option_name}=", option)
    end

    @options.type = Option.new({
        "name" => "type",
        "description" => "The component type",
        "example" => name,
        "null" => false,
        "type" => "string"
      })

    if type != "source"
      @options.inputs = Option.new({
        "name" => "inputs",
        "description" => "A list of upstream [source][sources] or [transform][transforms] IDs. See [Config Composition][config_composition] for more info.",
        "example" => ["my-source-id"],
        "null" => false,
        "type" => "string"
      })
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