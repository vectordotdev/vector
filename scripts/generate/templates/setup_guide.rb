class Templates
  class SetupGuide
    attr_reader :source, :sink

    def initialize(source: nil, sink: nil)
      if source.nil? && sink.nil?
        raise ArgumentError.new("You must supply at least a source or a sink")
      end

      @source = source
      @sink = sink
    end

    def action_phrase(narrative = :second_person)
      pronoun = narrative == :first_person ? "my" : "your"

      if source && sink
        "collect #{pronoun} #{source.title} #{source.event_types.collect(&:pluralize).to_sentence} and send them to #{sink.title}"
      elsif source
        "collect #{pronoun} #{source.title} #{source.event_types.collect(&:pluralize).to_sentence} and send them anywhere"
      elsif sink
        "send #{pronoun} #{sink.event_types.collect(&:pluralize).to_sentence} to #{sink.title}"
      end
    end

    def features
      @features ||=
        begin
          features = (source ? source.features : []) + (sink ? sink.features : [])

          if sink.nil?
            features << "Send your #{source.event_types.collect(&:pluralize).to_sentence} to one or more destinations"
          end

          features
        end
    end

    def tags
      @tags ||=
        begin
          strings = ["category: setup"]

          if source
            strings << "source: #{source.name}"
          end

          if sink
            strings << "sink: #{sink.name}"
          end

          strings
        end
    end
  end
end
