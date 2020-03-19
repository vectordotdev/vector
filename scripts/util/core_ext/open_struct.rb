class OpenStruct
  def any?
    to_h.values.any?
  end

  def to_a
    to_h.values
  end
end
