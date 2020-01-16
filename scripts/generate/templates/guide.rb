require_relative "../templates"

class Guide < Templates
  attr_reader :title,
    :description,
    :goal,
    :source,
    :sink,
    :keywords,
    :metadata,
    :event_from,
    :event_to,
    :supports_parsing,
    :needs_conversion

  ARCHIVE_SINKS = [
    "aws_s3",
  ]

  def initialize(root_dir, source, sink, metadata)
    super(root_dir, metadata)

    if ARCHIVE_SINKS.include?(sink.name)
      @title = "Archiving #{source.title} Events to #{sink.title}"
      @description = "Learn how to archive #{source.title} events to #{sink.title}."
      @goal = "read events from #{source.title} and archive them to #{sink.title}"
    else
      @title = "Writing #{source.title} Events to #{sink.title}"
      @description = "Learn how to send #{source.title} events to #{sink.title}."
      @goal = "read events from #{source.title} and write them to #{sink.title}"
    end

    @source = source
    @sink = sink
    @keywords = [ source.title.downcase, sink.title.downcase ]
    @metadata = metadata
    @event_from = source.event_types[0]
    @event_to = @event_from
    @needs_conversion = false
    @supports_parsing = @event_from == 'log'
   
    if ! (sink.event_types.include? @event_from)
      @event_to = sink.event_types[0]
      @needs_conversion = true
    end
  end

  def event_parser()
    metadata.components.detect { |tform| tform.name == "json_parser" }
  end

  def event_converter_type()
    if event_from == 'metric'
      'metric_to_log'
    else
      'log_to_metric'
    end
  end

  def event_converter()
    converter_type = event_converter_type
    metadata.components.detect { |tform| tform.name == converter_type }
  end

  def event_enricher_type()
    if event_to == 'metric'
      'add_tags' # TODO: Something cooler
    else
      'aws_ec2_metadata'
    end
  end

  def event_enricher()
    converter_type = event_enricher_type
    metadata.components.detect { |tform| tform.name == converter_type }
  end

  def with_input(component, input)
    new_component = Marshal.load( Marshal.dump(component) )
    new_component.options_list.detect { |opt| opt.name == "inputs" }.examples[0][0] = input
    new_component
  end

  def get_binding
    binding
  end
end