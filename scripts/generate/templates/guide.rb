require_relative "../templates"

class Guide < Templates
  attr_reader :title,
    :description,
    :source,
    :sink,
    :keywords,
    :metadata,
    :event_from,
    :event_to,
    :needs_parsing,
    :needs_conversion

  def initialize(root_dir, title, description, source, sink, metadata)
    super(root_dir, metadata)
    @title = title
    @description = description
    @source = source
    @sink = sink
    @keywords = [ source.title.downcase, sink.title.downcase ]
    @metadata = metadata
    @event_from = source.event_types[0]
    @event_to = @event_from
    @needs_conversion = false
    @needs_parsing = @event_from == 'log'
   
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