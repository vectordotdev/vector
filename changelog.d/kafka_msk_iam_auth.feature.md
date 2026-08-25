The `kafka` source and sink now support AWS MSK IAM authentication via the new `msk_iam` option. When configured with the AWS region of the MSK cluster, Vector authenticates using SASL `OAUTHBEARER` tokens signed with AWS credentials from the default credentials provider chain, and refreshes them automatically.

```yaml
sinks:
  msk:
    type: kafka
    bootstrap_servers: "b-1.mycluster.abc123.c2.kafka.us-west-2.amazonaws.com:9098"
    msk_iam:
      region: "us-west-2"
```

authors: jamesdangercarpenter
