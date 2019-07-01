class Output
  attr_reader :name, :body

  def initialize(hash)
    @name = hash.fetch("name")
    @body = hash.fetch("body")
  end
end