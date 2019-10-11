class Templates
  class OptionsTable
    attr_reader :options

    def initialize(options)
      @options = options
    end

    def categories
      @categories ||= options.collect(&:category).uniq
    end

    def grouped
      @grouped ||=
        options.group_by do |option|
          title = "**#{option.required? && option.default.nil? ? "REQUIRED" : "OPTIONAL"}**"

          if categories.length > 1
           "#{title} - #{option.category}"
          else
            title
          end
        end
    end

    def option_tags(option)
      tags = []

      if option.required?
        tags << "required"
      end

      if !option.default.nil?
        tags << "default: #{option.default.inspect}"
      elsif option.optional?
        tags << "no default"
      end

      if option.default.nil? && option.enum.nil? && option.examples.any?
        value = option.examples.first.inspect

        if value.length > 30
          tags << "example: (see above)"
        else
          tags << "example: #{value}"
        end
      end

      if option.enum
        escaped_values = option.enum.collect { |enum| enum.to_toml }
        if escaped_values.length > 1
          tags << "enum: #{escaped_values.to_sentence(two_words_connector: " or ")}"
        else
          tags << "must be: #{escaped_values.first}"
        end
      end

      if !option.unit.nil?
        tags << "unit: #{option.unit}"
      end

      tags
    end

    def option_description(option)
      description = option.description.strip

      if option.templateable?
        description << "This option supports dynamic values via [Vector's template syntax][docs.configuration#template-syntax]."
      end

      if option.relevant_when
        description << " Only relevant when #{option.relevant_when_kvs.to_sentence}"
      end

      description << "[[references:#{option.name}]]"

      tags = option_tags(option)
      if tags.any?
        tags_markdown = tags.collect { |tag| "`#{tag}`" }.join(" ")
        description << "<br />#{tags_markdown}"
      end

      description
    end
  end
end