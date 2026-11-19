export const DEFAULT_EXAMPLE = {
  id: "default",
  name: "Default",
  program: `# Parse a syslog line into structured fields. "!" aborts on a parsing error.
. = parse_syslog!(.message)

# Convert the parsed timestamp to Unix seconds.
.timestamp = to_unix_timestamp!(.timestamp)

# Remove fields we no longer need.
del(.facility)
del(.procid)

# Replace the message ID with a new UUID on each run.
.msgid = uuid_v4()

# Classify the message and add a field we can use for routing.
.critical = contains(string!(.message), "Great Scott!")
if .critical {
    .priority = "high"
} else {
    .priority = "normal"
}`,
  event: {
    message:
      "<29>1 1985-07-03T00:00:00Z acmecorp.biz authsvc 5265 ID486 - Great Scott! The flux capacitor needs attention."
  }
};

export const VRL_EXAMPLES = [
  DEFAULT_EXAMPLE,
  {
    id: "parse-json",
    name: "Parse JSON",
    program: `# Parse the message into a nested object, preserving the host field.
.http = parse_json!(.message)
del(.message)`,
    event: {
      message: '{"status":200,"path":"/checkout"}',
      host: "web-01"
    }
  },
  {
    id: "remove-sensitive-fields",
    name: "Remove sensitive fields",
    program: `# Remove fields at the root and inside a nested object.
del(.token)
del(.user.password)
.service = "checkout"`,
    event: {
      message: "User signed in",
      token: "example-token",
      user: { name: "alice", password: "example-password" }
    }
  },
  {
    id: "normalize-http-status",
    name: "Normalize HTTP status",
    program: `# Convert a string to an integer before comparing it.
.status = to_int!(.status)
if .status >= 200 && .status < 300 {
    .outcome = "success"
} else {
    .outcome = "error"
}`,
    event: {
      status: "200",
      path: "/checkout"
    }
  },
  {
    id: "parse-timestamp",
    name: "Parse a timestamp",
    program: `# Parse a timestamp with an explicit UTC offset.
.timestamp = parse_timestamp!(.time, format: "%+")
del(.time)`,
    event: {
      message: "Order received",
      time: "2026-01-15T10:30:00+02:00"
    }
  },
  {
    id: "match-health-checks",
    name: "Match health checks",
    program: `# Use contains() to match a substring, rather than an "in" operator.
# Store the result in the event so it appears in the output.
message = string(.message) ?? ""
.is_health_check = contains(message, "/health")`,
    event: {
      message: "GET /health HTTP/1.1",
      status: 200
    }
  },
  {
    id: "handle-parsing-errors",
    name: "Handle parsing errors",
    program: `# Capture the error instead of terminating the program with parse_json!().
# Try replacing the message with valid JSON, such as {"status":200}.
parsed, err = parse_json(.message)
if err == null {
    .parsed = parsed
} else {
    .parse_error = err
}`,
    event: {
      message: "not valid JSON",
      host: "web-01"
    }
  }
];
