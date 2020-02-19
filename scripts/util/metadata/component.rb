#encoding: utf-8

require_relative "field"

class Component
  DELIVERY_GUARANTEES = ["at_least_once", "best_effort"].freeze
  EVENT_TYPES = ["log", "metric"].freeze
  OPERATING_SYSTEMS = ["Linux", "MacOS", "Windows"].freeze

  include Comparable

  attr_reader :beta,
    :common,
    :env_vars,
    :function_category,
    :id,
    :name,
    :operating_systems,
    :options,
    :posts,
    :requirements,
    :service_providers,
    :title,
    :type,
    :unsupported_operating_systems

  def initialize(hash)
    @beta = hash["beta"] == true
    @common = hash["common"] == true
    @env_vars = (hash["env_vars"] || {}).to_struct_with_name(Field)
    @function_category = hash.fetch("function_category").downcase
    @name = hash.fetch("name")
    @posts = hash.fetch("posts")
    @requirements = hash["requirements"]
    @service_providers = hash["service_providers"] || []
    @title = hash.fetch("title")
    @type ||= self.class.name.downcase
    @id = "#{@name}_#{@type}"
    @options = (hash["options"] || {}).to_struct_with_name(Field)

    # Operating Systems

    if hash["only_operating_systems"]
      @operating_systems = hash["only_operating_systems"]
    elsif hash["except_operating_systems"]
      @operating_systems = OPERATING_SYSTEMS - hash["except_operating_systems"]
    else
      @operating_systems = OPERATING_SYSTEMS
    end

    @unsupported_operating_systems = OPERATING_SYSTEMS - @operating_systems
  end

  def <=>(other)
    name <=> other.name
  end

  def advanced_relevant?
    options_list.any?(&:advanced?)
  end

  def beta?
    beta == true
  end

  def common?
    common == true
  end

  def context_options
    options_list.select(&:context?)
  end

  def env_vars_list
    @env_vars_list ||= env_vars.to_h.values.sort
  end

  def event_types
    types = []

    if respond_to?(:input_types)
      types += input_types
    end

    if respond_to?(:output_types)
      types += output_types
    end

    types.uniq
  end

  def only_service_provider?(provider_name)
    service_providers.length == 1 && service_provider?(provider_name)
  end

  def options_list
    @options_list ||= options.to_h.values.sort
  end

  def option_groups
    @option_groups ||= options_list.collect(&:groups).flatten.uniq.sort
  end

  def option_example_groups
    @option_example_groups ||=
      begin
        groups = {}

        if option_groups.any?
          option_groups.each do |group|
            groups[group] = options_list.select do |option|
              option.group?(group) && option.common?
            end
          end

          if advanced_relevant?
            option_groups.each do |group|
              groups["#{group} (advanced)"] = options_list.select do |option|
                option.group?(group)
              end
            end
          end
        else
          groups["Common"] = options_list.select(&:common?)

          if advanced_relevant?
            groups["Advanced"] = options_list
          end
        end

        groups
      end
  end

  def partition_options
    options_list.select(&:partition_key?)
  end

  def service_provider?(provider_name)
    service_providers.collect(&:downcase).include?(provider_name.downcase)
  end

  def sink?
    type == "sink"
  end

  def sorted_option_group_keys
    option_example_groups.keys.sort_by do |key|
      if key.downcase.include?("advanced")
        -1
      else
        1
      end
    end.reverse
  end

  def source?
    type == "source"
  end

  def specific_options_list
    options_list.select do |option|
      !["type", "inputs"].include?(option.name)
    end
  end

  def status
    beta? ? "beta" : "prod-ready"
  end

  def templateable_options
    options_list.select(&:templateable?)
  end

  def to_h
    {
      beta: beta?,
      delivery_guarantee: (respond_to?(:delivery_guarantee, true) ? delivery_guarantee : nil),
      description: description,
      event_types: event_types,
      function_category: (respond_to?(:function_category, true) ? function_category : nil),
      id: id,
      name: name,
      operating_systems: (transform? ? [] : operating_systems),
      service_providers: service_providers,
      status: status,
      type: type,
      unsupported_operating_systems: unsupported_operating_systems
    }
  end

  def transform?
    type == "transform"
  end
end
