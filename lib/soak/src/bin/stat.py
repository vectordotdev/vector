#!/usr/bin/env python3

import codecs
import json

with codecs.open("/tmp/captures.txt") as file:
    for line in file:
        print(json.loads(line)['experiment'])
