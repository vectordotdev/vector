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

    def option_description(option)
      description = option.description.strip

      if option.templateable?
        description << "This option supports dynamic values via [Vector's template syntax][docs.configuration#template-syntax]."
      end

      if option.relevant_when
        description << " Only relevant when #{option.relevant_when_kvs.to_sentence(two_words_connector: " or ")}"
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