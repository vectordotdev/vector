class Permission
  include Comparable

  attr_reader :name,
    :description

  def initialize(hash)
    @name = hash.fetch("name")
    @description = hash.fetch("description")
  end

  def <=>(other)
    name <=> other.name
  end

  def eql?(other)
    self.<=>(other) == 0
  end

  def to_h
    {
      description: description,
      name: name
    }
  end
end
