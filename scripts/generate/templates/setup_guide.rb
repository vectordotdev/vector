require_relative "setup_guide"

class Templates
  class SetupGuide
    def initialize(title, interfaces, strategy, platform: nil, source: nil, sink: nil)
      if platform && source
        raise ArgumentError.new("You cannot provide both a platform and a source")
      end

      if platform.nil? && source.nil? && sink.nil?
        raise ArgumentError.new("You must supply at least a platform, source, or sink")
      end

      @interfaces = interfaces
      @platform = platform
      @source = source
      @sink = sink
      @strategy = source.strategies.first
      @title = title
    end

    def action_phrase(narrative = :second_person)
      pronoun = narrative == :first_person ? "my" : "your"

      if @source && @sink
        "collect #{pronoun} #{@title} #{@source.event_types.collect(&:pluralize).to_sentence} and send them to #{@sink.title}"
      elsif @source
        "collect #{pronoun} #{@title} #{@source.event_types.collect(&:pluralize).to_sentence} and send them anywhere"
      elsif @sink
        "send #{pronoun} #{@sink.event_types.collect(&:pluralize).to_sentence} to #{@title}"
      end
    end

    def features
      @features ||=
        begin
          features = (@source ? @source.features : []) + (@sink ? @sink.features : [])

          if @source.nil?
            features << "Collect your #{@sinks.event_types.collect(&:pluralize)} from one or more sources"
          end

          if @sink.nil?
            features << "Send your #{@source.event_types.collect(&:pluralize).to_sentence} to one or more destinations"
          end

          features
        end
    end

    def tags
      @tags ||=
        begin
          strings = ["category: setup"]

          if @platform
            strings << "platform: #{@platform.name}"
          end

          if @source
            strings << "source: #{@source.name}"
          end

          if @sink
            strings << "sink: #{@sink.name}"
          end

          strings
        end
    end
  end
end
