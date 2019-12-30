class CommitScope
  include Comparable

  attr_reader :name

  def initialize(name)
    @name = name
  end

  def <=>(other)
    if other.is_a?(self.class)
      name <=> other.name
    else
      nil
    end
  end

  def category
    @category ||= component_type || "core"
  end

  def component_name
    return @component_name if defined?(@component_name)

    @component_name =
      if !component_type.nil?
        component_name = name.split(" ").first

        if component_name != "new"
          component_name
        else
          nil
        end
      else
        nil
      end
  end

  def component_type
    return @component_type if defined?(@component_type)

    @component_type =
      if name.end_with?(" source")
        "source"
      elsif name.end_with?(" transform")
        "transform"
      elsif name.end_with?(" sink")
        "sink"
      else
        nil
      end
  end

  def new_component?
    !component_type.nil? && component_name.nil?
  end

  def eql?(other)
    self.<=>(other) == 0
  end

  def existing_component?
    !component_type.nil? && !component_name.nil?
  end

  def hash
    name.hash
  end

  def short_link
    return @short_link if defined?(@short_link)

    @short_link =
      case name
      when "cli"
        "docs.administration"
      when "config"
        "docs.configuration"
      when "log data model"
        "docs.data-model.log"
      when "metric data model"
        "docs.data-model.metric"
      when "observability"
        "docs.monitoring"
      else
        if component_name
          "docs.#{component_type.pluralize}.#{component_name}"
        else
          nil
        end
      end
  end

  def to_h
    {
      category: category,
      component_name: component_name,
      component_type: component_type,
      name: name
    }
  end
end
