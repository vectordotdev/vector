class Templates
  class ConfigSpec
    attr_reader :options

    def initialize(options)
      @options = options
    end

    def categories
      @categories ||= options.collect(&:category).uniq
    end

    def grouped
      @grouped ||= options.group_by(&:category)
    end
  end
end
