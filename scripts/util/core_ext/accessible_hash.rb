class AccessibleHash < Hash
  def initialize(hash)
    replace(hash)
  end

  private
    def method_missing(name, *args, &block)
      self.[](name) ||
        self.[](name.to_s) ||
        raise(NoMethodError.new("Method #{name} does not exist"))
    end

    def respond_to_missing?(name, include_private = false)
      key?(name) || key?(name.to_s)
    end
end
