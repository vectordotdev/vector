class Templates
  class IntegrationGuide
    def initialize(interfaces, strategy, platform: nil, source: nil, sink: nil)
      if platform.nil? && source.nil? && sink.nil?
        raise ArgumentError.new("You must supply at least a platform, source, or sink")
      end

      @interfaces = interfaces
      @platform = platform
      @source = source
      @sink = sink
      @strategy = strategy
    end

    def action_phrase(narrative = nil)
      pronoun =
        case narrative
        when :first_person
          "my "
        when :second_person
          "your "
        else
          nil
        end

      target =
        case narrative
        when :first_person
          "somewhere"
        else
          "anywhere"
        end

      if @source && @sink
        "send #{pronoun}#{@source.event_types.collect(&:pluralize).to_sentence} from #{normalize_title(@source.noun)} to #{normalize_title(@sink.noun)}"
      elsif @source
        "collect #{pronoun}#{@source.event_types.collect(&:pluralize).to_sentence} from #{normalize_title(@source.noun)} and send them #{target}"
      elsif @sink
        "send #{pronoun}#{@sink.event_types.collect(&:pluralize).to_sentence} to #{normalize_title(@sink.noun)}"
      end
    end

    def features
      @features ||=
        begin
          features = []

          if @source
            features << {
              text: @source.features[0],
              features: @source.features[1..-1].collect { |f| {text: f }.to_struct }
            }.to_struct
          else
            features << {
              text: "Collect your #{@sinks.event_types.collect(&:pluralize)} from one or more sources",
              features: []
            }.to_struct
          end

          if @sink
            features << {
              text: @sink.features[0],
              features: @sink.features[1..-1].collect { |f| {text: f }.to_struct }
            }.to_struct
          else
            features << {
              text: "Send your #{@source.event_types.collect(&:pluralize).to_sentence} to one or more destinations",
              features: []
            }.to_struct
          end

          features
        end
    end

    def tags
      @tags ||=
        begin
          # types first
          strings = ["type: tutorial"]

          # domains next
          if @platform
            strings << "domain: platforms"
          end

          if @source && !@platform
            strings << "domain: sources"
          end

          if @sink
            strings << "domain: sinks"
          end

          # types last
          if @platform
            strings << "platform: #{@platform.name}"
          end

          if @source && !@platform
            strings << "source: #{@source.name}"
          end

          if @sink
            strings << "sink: #{@sink.name}"
          end

          strings
        end
    end

    private
      def normalize_title(title)
        title.gsub(/ (logs|metrics)$/i, '')
      end
  end
end
