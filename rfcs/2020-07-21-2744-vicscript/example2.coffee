# Parses events of any structure:
#
# {
#     "nulled": null,
#     "basic": true,
#     "list": [
#         true,
#         null,
#         [true, null, true],
#         {
#             "basic": true,
#             "buddy": 1.0
#         }
#     ],
#     "map": {
#         "basic": true,
#         "list": [true, null, true],
#         "map": {
#             "basic": true,
#             "buddy": -1
#         }
#     }
# }
#
# And returns an object containing and array of all keys and a separate array of all values:
#
# {"keys":["map.map.buddy","list.2.0","map.list.0","list.0","basic","map.list.1","list.2.2","list.3.buddy","map.list.2","map.basic","list.2.1","nulled","list.3.basic","map.map.basic","list.1"],"values":[-1,true,true,true,true,null,true,1,true,true,null,null,true,true,null]}
#

root = this.collapse().map_each({"keys":key,"values":value}).values().fold(
  {"keys":[],"values":[]},
  {
    "keys": tally.keys.append(value.keys),
    "values": tally.values.append(value.values)
  }
)
