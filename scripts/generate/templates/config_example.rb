class Templates
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
  end
end
