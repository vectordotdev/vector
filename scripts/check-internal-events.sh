#!/usr/bin/env bash
exec find src/internal_events -type f -name \*.rs -exec awk '
  BEGIN {
    RS = ""
    FS = "\n"
    error_count = 0
  }

  match($0, /(trace|debug|info|warn|error)!\(\s*(message\s*=\s*)?"([^"]+)"/, groups) {
    message = groups[3]
    delete errors;
    if (!match(message, /^[A-Z]/)) { errors[1] = "Message must begin with a capital." }
    if (!match(message, /\.$/)) { errors[2] = "Message must end with a period." }
    if (length(errors)) {
      print FILENAME, ": Errors:"
      for (i in errors) {
        print "    ", errors[i]
      }
      print $0
      print ""
      error_count++
    }
  }

  END {
    print error_count, "error(s)!"
    if (error_count > 0) {
      exit 1
    }
  }
' {} +
