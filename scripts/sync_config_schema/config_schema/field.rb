class Field
  attr_reader :name,
    :config_option,
    :description,
    :example,
    :type

  def initialize(option_hash)
    @name = option_hash.fetch("name")
    @config_option = option_hash["config_option"]
    @description = option_hash.fetch("description")
    @example = option_hash.fetch("example")
    @type = option_hash.fetch("type")
  end
end