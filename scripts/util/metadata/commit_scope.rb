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

  def component
    return @component if defined?(@component)

    component_name = name.split(" ").first

    component_type =
      if name.end_with?(" source")

      elsif name.end_with?(" transform")
        "transform"
      elsif name.end_with?(" sink")
        "sink"
      else
        nil
      end

    @component =
      if component_name && component_type
        {name: component_name, type: component_type}.to_struct
      else
        nil
      end
  end

  def eql?(other)
    self.<=>(other) == 0
  end

  def hash
    name.hash
  end

  def to_h
    {
      component: component.deep_to_h,
      name: name
    }
  end
end
