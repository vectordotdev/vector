# NOTE: This example is intended to demonstrate larger applications for
# Vicscript in order to get a feel for the syntax. However, this uses features
# and concepts that aren't yet decided upon and aren't part of the RFC.
#
# Problem: My nan likes to send me CSV files embedded within JSON documents:
#
# {"items":"item,count\napples,10\noranges,2\n","doc":{"title":"shopping list","description":"get me this stuff"}}
#
# I want to parse and expand the csv doc within items like so:
#
# {
#   "doc":{
#     "title":"shopping list",
#     "description":"get me this stuff",
#     "items": [
#       {"item":"apples","count":10},
#       {"item":"oranges","count":2}
#     ]
#   }
# }
#
# Note: this example includes some of the more advanced mapping functions from
# Bloblang such as enumerated and map_each. In a real world scenario this
# example would be replaced with something bespoke like items.parse_csv().
#

# First, copy the unchanged contents of doc to our new event.
doc = doc

# Next, parse the csv out into an array of arrays to a temporary variable.
let rows = items.split("\n").map_each(match this.trim() {
    this.length() == 0 => deleted(), # Remove empty lines
    _ => this.split(","),
})

# The first row is column names
let column_names = $rows.0

# And here's the meaty part where we bring it all together. We walk each element
# of our array of arrays of values, and enumerate the value array. Then, using
# the index of the value, we create a temporary object with a key taken from the
# column_names variable and fold it.
doc.items = $rows.slice(1).map_each(
  this.enumerated().fold({}, tally.merge({
      $column_names.index(value.index): value.value
  }))
)
