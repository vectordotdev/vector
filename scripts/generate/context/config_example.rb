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
          title = "#{option.required? ? "REQUIRED" : "OPTIONAL"}"

          if categories.length > 1
           "#{title} - #{option.category}"
          else
            title
          end
        end
    end

    def tags(option)
      tags = []

      if !option.default.nil?
        tags << "default"
      elsif option.optional?
        tags << "no default"
      end

      if option.unit
        tags << option.unit
      end

      if option.enum
        if option.enum.length > 1
          tags << "enum: #{option.enum.collect(&:to_toml).join(", ")}"
        else
          tags << "must be: #{option.enum.first.to_toml}"
        end
      end

      tags
    end
  end
end