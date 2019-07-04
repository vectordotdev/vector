#encoding: utf-8

class Example
  attr_reader :name, :body

  def initialize(hash)
    @name = hash.fetch("name")
    @body = hash.fetch("body")
  end
end