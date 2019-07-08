class Object
  def is_primitive_type?
    is_a?(String) ||
      is_a?(Integer) ||
      is_a?(TrueClass) ||
      is_a?(FalseClass) ||
      is_a?(NilClass) ||
      is_a?(Float)
  end

  def to_toml
    if is_a?(Hash)
      values = collect { |key, value| "#{key} = #{value.to_toml}" }
      "{" + values.join(", ") + "}"
    elsif is_a?(Array)
      values = collect { |value| value.to_toml }
      "[" + values.join(", ") + "]"
    elsif is_a?(Time)
      iso8601(6)
    elsif is_a?(String) && include?("\n")
      <<~EOF
      """
      #{self}
      """
      EOF
    elsif is_primitive_type?
      inspect
    else
      raise "Unknown value type: #{self.class}"
    end
  end
end
