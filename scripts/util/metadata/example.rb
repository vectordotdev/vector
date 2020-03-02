class Example
  attr_reader :body, :label

  def initialize(hash)
    @body = hash.fetch("body")
    @label = hash.fetch("label")
  end
end
