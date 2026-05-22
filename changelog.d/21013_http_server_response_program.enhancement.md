Add the `response_source` config option to the `http_server` source, allowing a VRL program to generate a custom HTTP response. The program receives the decoded events as input (`.` is an array of event objects) and can return a string body or an object with `status`, `body`, and `headers` fields.

authors: stigglor
