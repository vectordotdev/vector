require_relative "log"
require_relative "metric"

class DataModel
  TYPES = ["log", "metric"].freeze

  attr_reader :log, :metric

  def initialize(hash)
    @log = Log.new(hash.fetch("log"))
    @metric = Metric.new(hash.fetch("metric"))
  end

  def types
    TYPES
  end
end
