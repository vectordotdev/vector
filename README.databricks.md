This lists custom changes merged in Databricks fork of Vector.
1. Fix premature ack when data file is full. https://github.com/databricks/vector/pull/1
2. Retry s3 request even when observing ConstructionFailure to avoid data loss. https://github.com/databricks/vector/pull/2
3. Updating/adding INFO logs for when vector sends to cloud storage. https://github.com/databricks/vector/pull/5
4. Allow retries on all sink exceptions https://github.com/databricks/vector/pull/7
5. Also allowing retries on AccessDenied exceptions in AWS https://github.com/databricks/vector/pull/12
6. Updating version to also carry a Databricks version https://github.com/databricks/vector/pull/13
7. Add a new event for successful upload to cloud storage (+ rework old send) https://github.com/databricks/vector/pull/14
8. Add new Vector events for topology events (new source/sink creation, vector start/stop) https://github.com/databricks/vector/pull/17
9. Provide an option to override the Content-Encoding header for files uploaded by Google Cloud Storage sink https://github.com/databricks/vector/pull/30
10. Add functionality to derive topic from file upload path https://github.com/databricks/vector/pull/33
11. Update event logs to support emitting granular upload events https://github.com/databricks/vector/pull/35
