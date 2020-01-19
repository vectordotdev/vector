class Tutorial
  class Step
    class Choice
      attr_reader :name, :steps, :title

      def initialize(hash)
        @name = hash.fetch("name")
        @steps = hash.fetch("steps").collect { |e| Tutorial::Step.new(e) }
        @title = hash.fetch("title")
      end

      def <=>(other)
        title <=> other.title
      end
    end

    attr_reader :choices, :description, :placeholder, :steps, :title, :type

    def initialize(hash)
      @choices = (hash["choices"] || {}).to_struct_with_name(Choice)
      @description = hash["description"]
      @placeholder = hash["placeholder"]

      @steps =
        if (hash["steps"] || []).any?
          hash.fetch("steps").collect { |e| self.class.new(e) }
        else
          []
        end

      @title = hash.fetch("title")
      @type = hash.fetch("type")
    end

    def choices_list
      @choices_list ||= choices.to_h.values.sort
    end
  end

  attr_reader :description, :steps, :variations

  def initialize(hash)
    @description = hash["description"]
    @steps = (hash["steps"] || []).collect { |e| Step.new(e) }
  end
end
