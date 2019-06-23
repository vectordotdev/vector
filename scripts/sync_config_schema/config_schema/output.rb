class Output
  attr_reader :type, :body

  def self.build(body)
    new({
      "type" => "*",
      "body" => body
    })
  end

  def initialize(hash)
    @type = hash.fetch("type")
    @body = hash.fetch("body")
  end
end