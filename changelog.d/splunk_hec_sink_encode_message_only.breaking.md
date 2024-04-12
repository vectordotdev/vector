The Splunk HEC sink is now using the `message` meaning to retrieve the relevant value and encodes the retrieved value
only. Note that if the retrieved value is `None`, an event with an empty message will be published.
