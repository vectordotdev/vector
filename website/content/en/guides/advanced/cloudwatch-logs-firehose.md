---
title: Ingesting AWS CloudWatch Logs via AWS Kinesis Firehose
description: Use CloudWatch Log subscriptions and Kinesis Firehose to robustly collect and route your CloudWatch logs.
authors: ["jszwedko", "spencergilbert"]
domains: ["sources", "transforms"]
transforms: ["remap"]
weight: 2
tags: ["aws", "cloudwatch", "logs", "firehose", "advanced", "guides", "guide"]
---

{{< requirement title="Pre-requisites" >}}

* You have logs in [AWS CloudWatch Logs][AWS CloudWatch Logs] that you'd like to
  consume.
* You are able to deploy Vector with a publicly exposed HTTPS endpoint.

[AWS CloudWatch Logs]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/WhatIsCloudWatchLogs.html
{{< /requirement >}}

If you use [AWS CloudWatch Logs][AWS CloudWatch Logs] on Amazon Web Services
(AWS), you may be wondering how to ingest these logs with Vector so you can
transform and send them to your service of choice. This guide will walk you
through a production-ready setup using [AWS Kinesis Firehose][AWS Kinesis
Firehose] to forward [AWS CloudWatch Logs][AWS CloudWatch Logs] to one or more
running Vector instances over HTTPS.

You will learn how to:

* Configure Vector to consume AWS CloudWatch Log events via the
 [`aws_kinesis_firehose`][aws_kinesis_firehose] source and
 [`remap`][remap]
 remap
* Configure [AWS Kinesis Firehose][AWS Kinesis Firehose] to forward events to
  a remotely running Vector instance (or instances)

Once completed, you'll have a setup that will receive events written to
CloudWatch Logs and writes them to standard out, but the configuration can be
adapted to send them to any other [destination of your choice](https://vector.dev/docs/reference/sinks/).

The goal of this guide is to serve as a reference that you can customize for
your specific needs.

## Motivation

Why use [AWS CloudWatch Logs subscriptions][AWS CloudWatch Logs subscriptions]
and [AWS Kinesis Firehose][AWS Kinesis Firehose] to forward logs to Vector? This
pipeline:

* Is tolerant of downtime
  * Firehose will retry requests for a configurable period
  * Firehose will dead-letter events that cannot be sent to S3
* Allows you to load balance the log ingestion
  * You can put multiple `vector` instances behind a load balancer
* Allows to ingest logs from multiple log groups
  * You can use one Firehose delivery stream as the destination for multiple
      CloudWatch Logs subscription filters

## Setup

Let's set some environment variables to make the below scripts more easily used
without edits.

```bash
# Update these
export AWS_ACCOUNT_ID="111111111111" # your AWS account ID
export AWS_REGION="us-east-1" # region that resources exist in
export FIREHOSE_ACCESS_KEY="my secret key" # set to a secret value
export VECTOR_ENDPOINT="https://example.com" # the endpoint where vector is deployed with the aws_kinesis_firehose source (see Configuring Vector below)

# Update these if needed
export LOG_GROUP="/test/vector" # the log group you want to consume with vector
export FIREHOSE_DELIVERY_STREAM="vector-stream"
export FIREHOSE_LOG_GROUP="/aws/kinesisfirehose/vector-stream" # log group for Kinesis Firehose to log messages to
export FIREHOSE_LOG_STREAM="HttpEndpointDelivery" # log stream in DEBUG_LOG_GROUP for Kinesis Firehose to log messages to
export FIREHOSE_S3_BUCKET="firehose-${AWS_ACCOUNT_ID}" # a bucket to write events that failed to be forwarded to
```

## Configuring Vector

Let's take a look at the configuration we will be using:

```toml title="vector.toml"
[sources.firehose]
  type = "aws_kinesis_firehose"
  address = "0.0.0.0:8080" # the public URL will be set when configuring Firehose
  access_key = "${FIREHOSE_ACCESS_KEY} # this will also be set when configuring Firehose

[transforms.parse]
  type = "remap"
  inputs = ["firehose"]
  drop_on_error = false
  source = '''
    parsed = parse_aws_cloudwatch_log_subscription_message!(.message)
    . = unnest(parsed.log_events)
    . = map_values(.) -> |value| {
       event = del(value.log_events)
       value |= event
       message = del(.message)
       . |= object!(parse_json!(message))
    }
  '''

# you may want to add more transforms here

[sinks.console]
  type = "console"
  inputs = ["parse"]
  encoding.codec = "json"
```

This will configure `vector` to listen for Firehose messages on the configured
port. These messages will then be transformed, via the
[`remap`][remap] transform to extract the individual log events, and then parse
these events. Finally, they are written to the console.

{{< info >}}
AWS Kinesis Firehose will only forward to HTTPS (and not HTTP) endpoints running
on port 443. You will need to either put a load balancer in front of the
`vector` instance to handle TLS termination or configure the `tls` parameters of
the `aws_kinesis_firehose` source to serve a valid certificate.
{{< /info >}}

Parts of the rest of this guide are based on [AWS's Subscription Filters with
Amazon Kinesis Data Firehose
guide](https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/SubscriptionFilters.html#FirehoseExample).
See their guide for more examples.

## Deploying Vector

First, deploy vector with the [`aws_kinesis_firehose`][aws_kinesis_firehose] source. See example
configuration above.

{{< info >}}
Remember that the port must be publicly accessible and the endpoint must be
serving HTTPS.
{{< /info >}}

## Creating a log groups

Let's create a new log group to use to ingest logs from. If the log group
already exists, you can skip this step.

```bash
$ aws logs create-log-group --log-group-name ${LOG_GROUP}
```

Additionally, we'll create a log group for Firehose to log to for debugging
purposes (see [Monitoring Kinesis Data Firehose Using CloudWatch
Logs](https://docs.aws.amazon.com/firehose/latest/dev/monitoring-with-cloudwatch-logs.html)
for more details).

```bash
$ aws logs create-log-group --log-group-name ${FIREHOSE_LOG_GROUP}
$ aws logs create-log-stream \
    --log-group-name ${FIREHOSE_LOG_GROUP} \
    --log-stream-name ${FIREHOSE_LOG_STREAM}
```

## Creating Kinesis Delivery Stream

Let's create the delivery stream to send CloudWatch Log event subscription to.
This will require several steps before we can create the stream itself.

## Create S3 bucket for events

The HTTP endpoint destination for Firehose requires you to have an S3 bucket
that it can write failed events too (after it retries them, see below for retry
configuration). Optionally, you can have it copy all events there rather than
only failed ones.

```bash
$ aws s3api create-bucket --bucket ${FIREHOSE_S3_BUCKET} \
    $(if [[ ${AWS_REGION} != "us-east-1" ]] ; then echo "--create-bucket-configuration LocationConstraint=${AWS_REGION}" ; fi)
```

## Create IAM role for delivery stream

We'll need to create an IAM role that will allow the delivery stream to write to
the bucket we just created.

```bash
$ aws iam create-role \
  --role-name FirehoseVector \
  --assume-role-policy-document file://<( cat <<EOF
{
  "Statement": {
    "Effect": "Allow",
    "Principal": {
      "Service": "firehose.amazonaws.com"
    },
    "Action": "sts:AssumeRole",
    "Condition": {
      "StringEquals": {
        "sts:ExternalId": "${AWS_ACCOUNT_ID}"
      }
    }
  }
}
EOF
)
```

Then we'll attach a policy allowing it to write to our bucket and debugging log
group.

```bash
$ aws iam put-role-policy --role-name FirehoseVector \
    --policy-name FirehoseVectorS3 \
    --policy-document file://<(cat <<EOF
{
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "s3:AbortMultipartUpload",
        "s3:GetBucketLocation",
        "s3:GetObject",
        "s3:ListBucket",
        "s3:ListBucketMultipartUploads",
        "s3:PutObject"
      ],
      "Resource": [
        "arn:aws:s3:::${FIREHOSE_S3_BUCKET}",
        "arn:aws:s3:::${FIREHOSE_S3_BUCKET}/*"
      ]
    },
    {
      "Effect": "Allow",
      "Action": [
        "logs:PutLogEvents"
      ],
      "Resource": [
        "arn:aws:logs:${AWS_REGION}:${AWS_ACCOUNT_ID}:log-group:${FIREHOSE_LOG_GROUP}:*"
      ]
    }
  ]
}
EOF
)
```

## Create stream

With the S3 bucket and IAM role in place, we can create our delivery stream.

```bash
$ aws firehose create-delivery-stream --delivery-stream-name ${FIREHOSE_DELIVERY_STREAM} \
    --http-endpoint-destination file://<(cat <<EOF
{
  "EndpointConfiguration": {
    "Url": "${VECTOR_ENDPOINT}",
    "Name": "vector",
    "AccessKey": "${FIREHOSE_ACCESS_KEY}"
  },
  "RequestConfiguration": {
    "ContentEncoding": "GZIP"
  },
  "CloudWatchLoggingOptions": {
    "Enabled": true,
    "LogGroupName": "${FIREHOSE_LOG_GROUP}",
    "LogStreamName": "${FIREHOSE_LOG_STREAM}"
  },
  "RoleARN": "arn:aws:iam::${AWS_ACCOUNT_ID}:role/FirehoseVector",
  "RetryOptions": {
    "DurationInSeconds": 300
  },
  "S3BackupMode": "FailedDataOnly",
  "S3Configuration": {
    "RoleARN": "arn:aws:iam::${AWS_ACCOUNT_ID}:role/FirehoseVector",
    "BucketARN": "arn:aws:s3:::${FIREHOSE_S3_BUCKET}"
  }
}
EOF
)
```

This will configure Firehose to ship all events to `$VECTOR_ENDPOINT` using the
provided access key. Any events that fail to be successfully processed after
300 seconds will be written to the S3 bucket for further processing.

See [aws firehose create-delivery-stream](https://docs.aws.amazon.com/cli/latest/reference/firehose/create-delivery-stream.html)
for more configuration options.

## Setup CloudWatch Logs subscription

We've got our delivery stream setup and pointing at our Vector instance(s). Let
us configure CloudWatch Logs subscription to forward events.

### Create IAM role for subscription

First we have to setup an IAM role and policy for CloudWatch Logs to write to
our Firehose delivery stream.

Let's create the IAM role.

```bash
$ aws iam create-role \
      --role-name CWLtoKinesisFirehoseRole \
      --assume-role-policy-document file://<(cat <<EOF
{
  "Statement": {
    "Effect": "Allow",
    "Principal": { "Service": "logs.${AWS_REGION}.amazonaws.com" },
    "Action": "sts:AssumeRole"
  }
}
EOF
)
`
```

Now we will attach a policy to our role to let it publish to the Firehose
stream.

```bash
$ aws iam put-role-policy --role-name CWLtoKinesisFirehoseRole \
    --policy-name Permissions-Policy-For-CWL \
    --policy-document file://<( cat <<EOF
{
    "Statement":[
      {
        "Effect":"Allow",
        "Action":["firehose:*"],
        "Resource":["arn:aws:firehose:${AWS_REGION}:${AWS_ACCOUNT_ID}:deliverystream/${FIREHOSE_DELIVERY_STREAM}"]
      }
    ]
}
EOF
)
```

### Create subscription

Now we can create the subscription.

```bash
$ aws logs put-subscription-filter \
  --log-group-name ${LOG_GROUP} \
  --filter-name "Destination" \
  --filter-pattern "" \
  --destination-arn "arn:aws:firehose:${AWS_REGION}:${AWS_ACCOUNT_ID}:deliverystream/${FIREHOSE_DELIVERY_STREAM}" \
  --role-arn "arn:aws:iam::${AWS_ACCOUNT_ID}:role/CWLtoKinesisFirehoseRole
```

{{< info >}}
You can set up multiple subscription filters sending to the same Firehose
delivery stream.
{{< /info >}}

## Testing it out

To make sure everything is wired up correctly, let's send some logs to the log
group we've setup. We can use Vector for this too!

```bash
[sources.stdin]
  type = "stdin"
[sinks.cloudwatch]
  type = "aws_cloudwatch_logs"
  inputs = ["stdin"]
  group_name = "${LOG_GROUP}"
  stream_name = "test"
  region = "us-east-1"
  encoding.codec = "json"
```

This will read lines from `stdin` and write them to CloudWatch Logs. See the
[AWS Authentication][aws_auth] section for this sink to see how to configure the
AWS credentials Vector will need to write to AWS.

Alternatively, you can publish events directly to a CloudWatch Log group through
the AWS console. First create a log stream within the group, click into it, and
you should see an option under Actions to "Create a log event"

Let's send some logs. For this, I'm using
[flog], a useful tool for generating fake
log data.

```bash
flog -f json | vector --config config.toml
```

This will send some logs to your log group. Within 300 seconds (the default
batch interval) these logs should be forwarded to your running `vector`
instance(s).

## Wrap Up

Congratulations, now you are ingesting logs from AWS CloudWatch Logs in a robust
manner. You can now use Vector to transform and forward these logs to your
destinations of choice.

Any troubles getting this to work? Let us know via [GitHub
issues](https://github.com/vectordotdev/vector/issues/new/choose) or [drop into the
`#aws` channel on our discord server.](https://chat.vector.dev)

Still want more? Read on for a deep dive into this pipeline.

## Deep dive

For the curious, let's take a look at what the events look like as they pass
through this pipeline. This can be useful if you'd like to inject different
transforms at various points.

Let's imagine that we publish the following two events to the CloudWatch Log
group, `/test/vector` that we had setup:

```json
{
  "bytes": 26780,
  "datetime": "14/Sep/2020:11:45:41 -0400",
  "host": "157.130.216.193",
  "method": "PUT",
  "protocol": "HTTP/1.0",
  "referer": "https://www.principalcross-platform.io/markets/ubiquitous",
  "request": "/expedite/convergence",
  "source_type": "stdin",
  "status": 301,
  "user-identifier": "-"
}
{
  "bytes": 17707,
  "datetime": "14/Sep/2020:11:45:41 -0400",
  "host": "109.81.244.252",
  "method": "GET",
  "protocol": "HTTP/2.0",
  "referer": "http://www.investormission-critical.io/24/7/vortals",
  "request": "/scale/functionalities/optimize",
  "source_type": "stdin",
  "status": 502,
  "user-identifier": "feeney1708"
}
```

These events will be forwarded via the CloudWatch Logs subscription we setup to
Kinesis Firehose. Kinesis Firehose will then encode the subscription event and
send it as an HTTP request:

```json
{
  "requestId": "ed1d787c-b9e2-4631-92dc-8e7c9d26d804",
  "timestamp": 1600110760138,
  "records": [
    {
      "data": "H4sIAMeba18AA52TX2/aMBTF3/spUJ4h/h/beUMqYy+TKsGexlSFcGm9JXFqO2Vd1e8+O7AiTUNMy0Ok3HNybN+f7+vNZJK14H31AOuXHrJykt3O1/P7T4vVar5cZNNksIcOXJKwJFpozqQg7Cg19mHp7NAnFX2LQYAC+PAuroKDqk3queyHra+d6YOx3QfTBHA+Gr5EKYq30Wa6KmlZrHz9HbR4hi6cfa/jO0pml8KZKBQrhMJKF4QLRTllBeZMc60YLbBkSlOqlBBEx0dIRaVQHI8bGnOCiW0IVZtOQgqMCcGi0Jjpd8epTWm51022fYkH2mQlLaTC0022qwKkjFjaZISjFfSIYopLQkouSk4mM8wx3mTR+2h9OPqEzAnDOSVFTjQbxRbCo92N8t3n9VjqnQ22ts1Y/Lhe3yGSH5Mc7MGBG4XHEHpfInQ4HPLema42fdXUzno/65sq7K1rc2NRW7nvEDwatuZpMMEO/pT0NMBpWwh+9LAzAVBtu2dwD9DVMLq8HVwN9yFeldHpw850RyVUIUWVDJP4OXhwM7OLzMzenDY422Rv2djNt+k1iEITxTSJHYs4C0q14EwRzNLtw4oUklKhcYRcSHYVIidXIBIpsfxviFjniuSU85wK+ifD5eISQ3qB4QmhiZ33IUIz3sdhmMWJCaaumsSQciTRs3Whav5Cz0cXoP3Q1WmKqib+Bx7ZOG+t+fnPHAWmFzjuATp4IRKrM9A0qjdvN78A1L2XllAEAAA="
    }
  ]
}
```

The [`aws_kinesis_firehose`][aws_kinesis_firehose] source:

```toml
[sources.firehose]
  type = "aws_kinesis_firehose"
  address = "0.0.0.0:8080" # the public URL will be set in the Firehose config
  access_key = "my secret key" # this will also be set in the Firehose config
```

will accept this request, decode the record (which is gzip'd and then base64 encoded), to produce an event that looks like:

```json
{
  "message": "{\n  \"messageType\": \"DATA_MESSAGE\",\n  \"owner\": \"111111111111\",\n  \"logGroup\": \"/test\",\n  \"logStream\": \"test\",\n  \"subscriptionFilters\": [\n    \"Destination\"\n  ],\n  \"logEvents\": [\n    {\n      \"id\": \"35683658089614582423604394983260738922885519999578275840\",\n      \"timestamp\": 1600110569039,\n      \"message\": \"{\\\"bytes\\\":26780,\\\"datetime\\\":\\\"14/Sep/2020:11:45:41 -0400\\\",\\\"host\\\":\\\"157.130.216.193\\\",\\\"method\\\":\\\"PUT\\\",\\\"protocol\\\":\\\"HTTP/1.0\\\",\\\"referer\\\":\\\"https://www.principalcross-platform.io/markets/ubiquitous\\\",\\\"request\\\":\\\"/expedite/convergence\\\",\\\"source_type\\\":\\\"stdin\\\",\\\"status\\\":301,\\\"user-identifier\\\":\\\"-\\\"}\"\n    },\n    {\n      \"id\": \"35683658089659183914001456229543810359430816722590236673\",\n      \"timestamp\": 1600110569041,\n      \"message\": \"{\\\"bytes\\\":17707,\\\"datetime\\\":\\\"14/Sep/2020:11:45:41 -0400\\\",\\\"host\\\":\\\"109.81.244.252\\\",\\\"method\\\":\\\"GET\\\",\\\"protocol\\\":\\\"HTTP/2.0\\\",\\\"referer\\\":\\\"http://www.investormission-critical.io/24/7/vortals\\\",\\\"request\\\":\\\"/scale/functionalities/optimize\\\",\\\"source_type\\\":\\\"stdin\\\",\\\"status\\\":502,\\\"user-identifier\\\":\\\"feeney1708\\\"}\"\n    }\n  ]\n}\n",
  "request_id": "ed1d787c-b9e2-4631-92dc-8e7c9d26d804",
  "source_arn": "arn:aws:firehose:us-east-1:111111111111:deliverystream/test",
  "timestamp": "2020-09-14T19:12:40.138Z"
}
```

Here we can see the some context fields have been extracted from the Firehose
source including the `request_id`, `source_arn` (passed as an HTTP header by
Firehose) and `timestamp` (parsed from the `timestamp` field of the request
body). The `message` field is the decoded record data. In our case, this is an
AWS CloudWatch Logs Subscription event.

To extract and parse the originating events from this subscription event, we
can use a [`remap`][remap] transform and leverage the [`parse_aws_cloudwatch_log_subscription_message`][parse_aws_cloudwatch_log_subscription_message],
[`unnest`][unnest], [`map_values`][map_values], and [`parse_json`][parse_json]
functions:

```toml
[transforms.parse]
  type = "remap"
  inputs = ["firehose"]
  drop_on_error = false
  source = '''
    parsed = parse_aws_cloudwatch_log_subscription_message!(.message)
    . = unnest(parsed.log_events)
    . = map_values(.) -> |value| {
       event = del(value.log_events)
       value |= event
       message = del(.message)
       . |= object!(parse_json!(message))
    }
  '''
```

Let's step through this program one function at a time.

```coffee
parsed = parse_aws_cloudwatch_log_subscription_message!(.message)
```

This line will parse the `.message` field and store the results in a variable
`parsed`. The contents of that variable would look like:

```json
{
  "messageType": "DATA_MESSAGE",
  "owner": "111111111111",
  "logGroup": "/test/vector",
  "logStream": "test",
  "subscriptionFilters": [
    "Destination"
  ],
  "logEvents": [
    {
      "id": "35683658089614582423604394983260738922885519999578275840",
      "timestamp": 1600110569039,
      "message": "{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-platform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}"
    },
    {
      "id": "35683658089659183914001456229543810359430816722590236673",
      "timestamp": 1600110569041,
      "message": "{\"bytes\":17707,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"109.81.244.252\",\"method\":\"GET\",\"protocol\":\"HTTP/2.0\",\"referer\":\"http://www.investormission-critical.io/24/7/vortals\",\"request\":\"/scale/functionalities/optimize\",\"source_type\":\"stdin\",\"status\":502,\"user-identifier\":\"feeney1708\"}"
    }
  ]
}
```

```coffee
. = unnest(parsed.log_events)
```

This will take the above event and output two new events:

```json
{
  "log_events": {
    "id": "35683658089614582423604394983260738922885519999578275840",
    "message": "{\"bytes\":26780,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"157.130.216.193\",\"method\":\"PUT\",\"protocol\":\"HTTP/1.0\",\"referer\":\"https://www.principalcross-platform.io/markets/ubiquitous\",\"request\":\"/expedite/convergence\",\"source_type\":\"stdin\",\"status\":301,\"user-identifier\":\"-\"}",
    "timestamp": "2020-09-14T19:09:29.039Z"
  },
  "log_group": "/test/vector",
  "log_stream": "test",
  "owner": "071959437513",
  "request_id": "ed1d787c-b9e2-4631-92dc-8e7c9d26d804",
  "source_arn": "arn:aws:firehose:us-east-1:111111111111:deliverystream/test",
  "subscription_filters": [
    "Destination"
  ],
}
{
  "log_events": {
    "id": "35683658089659183914001456229543810359430816722590236673",
    "message": "{\"bytes\":17707,\"datetime\":\"14/Sep/2020:11:45:41 -0400\",\"host\":\"109.81.244.252\",\"method\":\"GET\",\"protocol\":\"HTTP/2.0\",\"referer\":\"http://www.investormission-critical.io/24/7/vortals\",\"request\":\"/scale/functionalities/optimize\",\"source_type\":\"stdin\",\"status\":502,\"user-identifier\":\"feeney1708\"}",
    "timestamp": "2020-09-14T19:09:29.041Z"
  },
  "log_group": "/test/vector",
  "log_stream": "test",
  "owner": "071959437513",
  "request_id": "ed1d787c-b9e2-4631-92dc-8e7c9d26d804",
  "source_arn": "arn:aws:firehose:us-east-1:111111111111:deliverystream/test",
  "subscription_filters": [
    "Destination"
  ],
}
```

Here we can see the individual events are extracted along with some additional
context:

* `id`: the ID of the log event
* `log_group` the log_group of the log event
* `log_stream` the log_stream of the log event
* `owner` the AWS account ID of the owner of the log event
* `subscription_filters` the filters that matched to send the event
* `timestamp` is overwritten with the timestamp from the log event

This is pretty good, but, our original events are also JSON and the `.id`,
`.message`, and `.timestamp` fields are nested. The next function uses
iteration to finish up our processing.

```coffee
. = map_values(.) -> |value| {
   event = del(value.log_events)
   value |= event
   message = del(.message)
   . |= object!(parse_json!(message))
}
```

This snippet will iterate through the values in the root of the event,
the two objects shown above. For each "value" (an object) we delete the
`.log_events` field, saving it's contents in a variable `event`. We then
merge the contents into `value`, which moves the `.id`, `.message`,
and `.timestamp` fields into the root of that object.

We then delete the `.message` field, storing the contents in a variable
`message`. Finally we parse the variable as JSON, and merge the now structured
fields into the root of the object.

This will give us the final result of the following two events:

```json
{
  "bytes": 26780,
  "datetime": "14/Sep/2020:11:45:41 -0400",
  "host": "157.130.216.193",
  "id": "35683658089614582423604394983260738922885519999578275840",
  "log_group": "/test/vector",
  "log_stream": "test",
  "method": "PUT",
  "owner": "071959437513",
  "protocol": "HTTP/1.0",
  "referer": "https://www.principalcross-platform.io/markets/ubiquitous",
  "request": "/expedite/convergence",
  "request_id": "ed1d787c-b9e2-4631-92dc-8e7c9d26d804",
  "source_arn": "arn:aws:firehose:us-east-1:071959437513:deliverystream/jesse-test",
  "source_type": "stdin",
  "status": 301,
  "subscription_filters": [
    "Destination"
  ],
  "timestamp": "2020-09-14T19:09:29.039Z",
  "user-identifier": "-"
}
{
  "bytes": 17707,
  "datetime": "14/Sep/2020:11:45:41 -0400",
  "host": "109.81.244.252",
  "id": "35683658089659183914001456229543810359430816722590236673",
  "log_group": "/test/vector",
  "log_stream": "test",
  "method": "GET",
  "owner": "071959437513",
  "protocol": "HTTP/2.0",
  "referer": "http://www.investormission-critical.io/24/7/vortals",
  "request": "/scale/functionalities/optimize",
  "request_id": "ed1d787c-b9e2-4631-92dc-8e7c9d26d804",
  "source_arn": "arn:aws:firehose:us-east-1:071959437513:deliverystream/jesse-test",
  "source_type": "stdin",
  "status": 502,
  "subscription_filters": [
    "Destination"
  ],
  "timestamp": "2020-09-14T19:09:29.041Z",
  "user-identifier": "feeney1708"
}
```

Here are original events with all of the additional context!

[aws_auth]: /docs/reference/configuration/sinks/aws_cloudwatch_logs/#aws-authentication
[AWS CloudWatch Logs subscriptions]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Subscriptions.html
[aws_kinesis_firehose]: /docs/reference/configuration/sources/aws_kinesis_firehose/
[AWS CloudWatch Logs]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/WhatIsCloudWatchLogs.html
[AWS Kinesis Firehose]: https://aws.amazon.com/kinesis/data-firehose/?kinesis-blogs.sort-by=item.additionalFields.createdDate&kinesis-blogs.sort-order=desc
[flog]: https://github.com/mingrammer/flog
[parse_aws_cloudwatch_log_subscription_message]: /docs/reference/vrl/functions/#parse_json
[parse_json]: /docs/reference/vrl/functions/#parse_json
[remap]: /docs/reference/configuration/transforms/remap/
