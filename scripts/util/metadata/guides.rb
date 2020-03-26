class Guides
  attr_reader :description, :name, :title

  def initialize(hash)
    @description = hash.fetch("description")
    @name = hash.fetch("name")
    @title = hash.fetch("title")
  end

  def to_h
    {
      description: description,
      name: name,
      title: title
    }
  end
end
