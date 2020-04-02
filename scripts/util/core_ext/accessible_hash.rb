class AccessibleHash < Hash
  def initialize(hash)
    replace(hash)
  end

  def inspect(*args)
    "AccessibleHash<#{super}>"
  end

  private
    def method_missing(name, *args, &block)
      if key?(name)
        self[name]
      elsif key?(name.to_s)
        self[name.to_s]
      else
        raise(NoMethodError.new("Method `#{name}` does not exist on:\n\n  #{inspect}"))
      end
    end

    def respond_to_missing?(name, include_private = false)
      key?(name) || key?(name.to_s)
    end
end
