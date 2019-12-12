class String
  # This method should mimic the Github sluggify logic. This is required
  # to properly link to sections. Docusaurus uses this package:
  #
  # https://github.com/Flet/github-slugger/blob/master/index.js
  #
  # And this method is intended to mimic that.
  def slugify
    self.downcase.gsub(/[^a-z_\d\s-]/, '').gsub(" ", "-")
  end
end