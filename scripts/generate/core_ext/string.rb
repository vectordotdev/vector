require 'word_wrap/core_ext'

class String
  def commentify
    "# " + self.gsub("\n", "\n# ")
  end

  def continuize
    self[0] = self[0].downcase
    self
  end

  def editorify(width = 80)
    self.
      remove_markdown_links.
      wrap(width)
  end

  def remove_markdown_links
    self.
      gsub(/\[([^\]]+)\]\(([^) ]+)\)/, '\1').
      gsub(/\[([^\]]+)\]\[([^) ]+)\]/, '\1')
  end

  def replace(match, sub)
    gsub match, match => sub
  end

  def replace!(match, sub)
    gsub! match, match => sub
  end
end
