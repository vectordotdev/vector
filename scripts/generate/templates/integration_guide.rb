class Templates
  class IntegrationGuide
    attr_reader :event_types, :platform, :sink, :source

    def initialize(strategy, platform: nil, source: nil, sink: nil)
      if platform.nil? && source.nil? && sink.nil?
        raise ArgumentError.new("You must supply at least a platform, source, or sink")
      end

      @event_types =
        if source && sink
          source.event_types & sink.event_types
        elsif source
          source.event_types
        elsif sink
          sink.event_types
        else
          []
        end

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
        "send #{pronoun}#{@event_types.collect(&:pluralize).to_sentence} from #{noun_link(@source)} to #{noun_link(@sink)}"
      elsif @source
        "collect #{pronoun}#{@event_types.collect(&:pluralize).to_sentence} from #{noun_link(@source)} and send them #{target}"
      elsif @sink
        "send #{pronoun}#{@event_types.collect(&:pluralize).to_sentence} to #{noun_link(@sink)}"
      end
    end

    def cover_label
      @cover_label ||=
        if @platform && @sink
          "#{@platform.title} to #{@sink.title} Integration"
        elsif @platform
          "#{@platform.title} Integration"
        elsif @source && @sink
          "#{@source.title} to #{@sink.title} Integration"
        elsif @source
          "#{@source.title} Integration"
        elsif @sink.title
          "#{@sink.title} Integration"
        end
    end


    def description_count
      @description_count ||=
        begin
          count = 0

          if (platform && platform.description) || (source && source.description)
            count += 1
          end

          if sink && sink.description
            count += 1
          end

          count
        end
    end

    def events_phrase
      @events_phrase ||= event_types.collect(&:pluralize).to_sentence
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
              text: "Collect your #{@event_types.collect(&:pluralize).to_sentence} from one or more sources",
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
              text: "Send your #{@event_types.collect(&:pluralize).to_sentence} to one or more destinations",
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

          if @source
            strings << "source: #{@source.name}"
          end

          if @sink
            strings << "sink: #{@sink.name}"
          end

          strings
        end
    end

    private
      def noun_link(component)
        case component.name
        when "blackhole", "vector"
          return normalize_noun(component.noun)
        else
          "[#{normalize_noun(component.noun)}][#{short_link(component.name)}]"
        end
      end

      def short_link(name)
        "urls." + name.gsub(/_(logs|metrics)$/i, '')
      end

      def normalize_noun(title)
        title.gsub(/ (logs|metrics)$/i, '')
      end
  end
end
