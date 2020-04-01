require 'word_wrap/core_ext'

class String
  def capitalize_first
    self[0].capitalize + self[1..-1]
  end

  # Comments out a block of text
  def commentify
    "# " + self.gsub("\n", "\n# ")
  end

  # Downcases the first letter, even if it has markdown syntax
  def continuize
    i = 0

    loop do
      if i > self.length
        break
      end

      if self[i] != "["
        self[i] = self[i].downcase
        break
      end

      i = i+1
    end

    self
  end

  def editorify(width = 80)
    self.
      remove_markdown_links.
      wrap(width)
  end

  def html_escape
    ERB::Util.html_escape(self)
  end

  def humanize
    ActiveSupport::Inflector.humanize(self).
      gsub(/\bansi\b/i, 'ANSI').
      gsub(/\baws\b/i, 'AWS').
      gsub(/\bcloudwatch\b/i, 'Cloudwatch').
      gsub(/\bec2\b/i, 'EC2').
      gsub(/\bgcp\b/i, 'GCP').
      gsub(/\bhec\b/i, 'HEC').
      gsub(/\bhttp\b/i, 'HTTP').
      gsub(/\bjson\b/i, 'JSON').
      gsub(/\bkinesis\b/i, 'Kinesis').
      gsub(/\blua\b/i, 'LUA').
      gsub(/\bs3\b/i, 'S3').
      gsub(/\btcp\b/i, 'TCP').
      gsub(/\budp\b/i, 'UDP')
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

  # This method should mimic the Github sluggify logic. This is required
  # to properly link to sections. Docusaurus uses this package:
  #
  # https://github.com/Flet/github-slugger/blob/master/index.js
  #
  # And this method is intended to mimic that.
  def slugify
    self.downcase.gsub(/[^a-z_\d\s-]/, '').gsub(" ", "-")
  end

  def table_escape
    gsub("|", '\|')
  end
end
