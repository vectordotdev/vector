Updated the Splunk HEC source to accept requests that contain the header content-type with any value containing "application/json," not the exact value of "application/json." This matches the behavior of a true Splunk HEC. Allows sources from AWS to successfully send events to the Splunk HEC source without additional proxying to update headers.

authors: Tot19
