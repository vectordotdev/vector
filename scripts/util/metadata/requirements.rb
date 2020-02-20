class Requirements
  attr_accessor :additional
  attr_reader :network

  def initialize(hash)
    @additional = hash["additional"]
    @network = hash["network"] || false
  end

  def any?
    !additional.nil? || network
  end
end
