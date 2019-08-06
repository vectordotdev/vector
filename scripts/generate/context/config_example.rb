class Context
  class ConfigExample
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
          title = "#{option.required? && option.default.nil? ? "REQUIRED" : "OPTIONAL"}"

          if categories.length > 1
           "#{title} - #{option.category}"
          else
            title
          end
        end
    end

    def tags(option)
      tags = []

      if option.examples.first == option.default
        tags << "default"
      elsif option.default.nil? && option.optional?
        tags << "no default"
      end

      if option.unit
        tags << option.unit
      end

      if option.enum
        if option.enum.length > 1
          tags << "enum: #{option.enum.collect(&:to_toml).to_sentence(two_words_connector: " or ")}"
        else
          tag = "must be: #{option.enum.first.to_toml}"
          if option.optional?
            tag << " (if supplied)"
          end
          tags << tag
        end
      end

      if option.relevant_when
        conditions = option.relevant_when.collect { |k,v| "#{k} = #{v.to_toml}" }.to_sentence
        tag = "relevant when #{conditions}"
        tags << tag
      end

      tags
    end
  end
end