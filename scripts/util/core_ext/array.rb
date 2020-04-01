class Array
  def intersection(other)
    self & other
  end

  def promote(promoted_element)
    return self unless (found_index = find_index(promoted_element))
    unshift(delete_at(found_index))
  end
end
