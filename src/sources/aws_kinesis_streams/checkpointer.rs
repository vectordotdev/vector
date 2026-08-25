//! DynamoDB-backed shard checkpoint and lease management.
//!
//! Each shard checkpoint is a DynamoDB item with:
//!   - `StreamID` (hash key, String) — stream name or ARN used as the coordination key
//!   - `ShardID`  (range key, String)
//!   - `SequenceNumber` (String) — the last fully-acknowledged Kinesis sequence number
//!   - `ClientID` (String, optional) — UUID of the consumer that currently owns the shard
//!   - `LeaseTimeout` (String, optional) — RFC3339 timestamp until which the lease is valid

use std::collections::HashMap;
use std::time::Duration;

use aws_sdk_dynamodb::{
    Client as DynamoDbClient,
    error::DisplayErrorContext,
    types::{
        AttributeDefinition, AttributeValue, BillingMode, KeySchemaElement, KeyType,
        ProvisionedThroughput, ReturnValue, ScalarAttributeType,
    },
};
use chrono::{DateTime, Utc};

use crate::sources::aws_kinesis_streams::DynamoDbCheckpointConfig;

/// Error type for checkpointing operations.
#[derive(Debug)]
pub enum CheckpointerError {
    LeaseNotAcquired,
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl std::fmt::Display for CheckpointerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckpointerError::LeaseNotAcquired => {
                write!(f, "the shard could not be leased due to a collision")
            }
            CheckpointerError::Other(e) => write!(f, "DynamoDB error: {e}"),
        }
    }
}

impl std::error::Error for CheckpointerError {}

/// A single claimed shard for a given client.
#[derive(Debug, Clone)]
pub struct ClientClaim {
    pub shard_id: String,
    pub lease_timeout: DateTime<Utc>,
}

/// The full checkpoint state for a stream retrieved in one DynamoDB Query.
#[derive(Debug)]
pub struct CheckpointData {
    /// All shard IDs that have a checkpoint row (regardless of who owns them).
    pub shards_with_checkpoints: HashMap<String, bool>,
    /// Last checkpointed sequence per shard (`SHARD_END` when fully consumed).
    pub sequence_numbers: HashMap<String, String>,
    /// Map from client_id → list of claimed shards.
    pub client_claims: HashMap<String, Vec<ClientClaim>>,
}

/// A decoded checkpoint row.
#[derive(Debug)]
pub struct CheckpointItem {
    pub sequence_number: String,
    pub client_id: Option<String>,
    pub lease_timeout: Option<DateTime<Utc>>,
}

/// Manages DynamoDB-backed checkpoints and distributed leases for Kinesis shards.
#[derive(Clone)]
pub struct KinesisCheckpointer {
    pub client_id: String,
    conf: DynamoDbCheckpointConfig,
    lease_duration: Duration,
    svc: DynamoDbClient,
}

impl KinesisCheckpointer {
    /// Create a new checkpointer, optionally ensuring the DynamoDB table exists.
    pub async fn new(
        svc: DynamoDbClient,
        client_id: String,
        conf: DynamoDbCheckpointConfig,
        lease_duration: Duration,
    ) -> crate::Result<Self> {
        let c = KinesisCheckpointer {
            client_id,
            conf,
            lease_duration,
            svc,
        };
        c.ensure_table_exists().await?;
        Ok(c)
    }

    /// Verify the table exists; if `conf.create` is true, create it if missing.
    async fn ensure_table_exists(&self) -> crate::Result<()> {
        match self
            .svc
            .describe_table()
            .table_name(&self.conf.table)
            .send()
            .await
        {
            Ok(_) => return Ok(()),
            Err(e) => {
                let is_not_found = e
                    .as_service_error()
                    .map(|se| se.is_resource_not_found_exception())
                    .unwrap_or(false);
                if !is_not_found {
                    return Err(format!(
                        "DynamoDB DescribeTable error: {}",
                        DisplayErrorContext(&e)
                    )
                    .into());
                }
            }
        }

        if !self.conf.create {
            return Err(format!(
                "DynamoDB table '{}' does not exist and `create` is false",
                self.conf.table
            )
            .into());
        }

        let billing_mode = if self.conf.billing_mode == "PROVISIONED" {
            BillingMode::Provisioned
        } else {
            BillingMode::PayPerRequest
        };

        let mut builder = self
            .svc
            .create_table()
            .table_name(&self.conf.table)
            .billing_mode(billing_mode)
            .attribute_definitions(
                AttributeDefinition::builder()
                    .attribute_name("StreamID")
                    .attribute_type(ScalarAttributeType::S)
                    .build()
                    .map_err(|e| format!("Failed to build AttributeDefinition: {e}"))?,
            )
            .attribute_definitions(
                AttributeDefinition::builder()
                    .attribute_name("ShardID")
                    .attribute_type(ScalarAttributeType::S)
                    .build()
                    .map_err(|e| format!("Failed to build AttributeDefinition: {e}"))?,
            )
            .key_schema(
                KeySchemaElement::builder()
                    .attribute_name("StreamID")
                    .key_type(KeyType::Hash)
                    .build()
                    .map_err(|e| format!("Failed to build KeySchemaElement: {e}"))?,
            )
            .key_schema(
                KeySchemaElement::builder()
                    .attribute_name("ShardID")
                    .key_type(KeyType::Range)
                    .build()
                    .map_err(|e| format!("Failed to build KeySchemaElement: {e}"))?,
            );

        if self.conf.billing_mode == "PROVISIONED" {
            builder = builder.provisioned_throughput(
                ProvisionedThroughput::builder()
                    .read_capacity_units(self.conf.read_capacity_units)
                    .write_capacity_units(self.conf.write_capacity_units)
                    .build()
                    .map_err(|e| format!("Failed to build ProvisionedThroughput: {e}"))?,
            );
        }

        builder.send().await.map_err(|e| {
            format!(
                "Failed to create DynamoDB table: {}",
                DisplayErrorContext(&e)
            )
        })?;
        Ok(())
    }

    /// Fetch a single checkpoint item.
    pub async fn get_checkpoint(
        &self,
        stream_id: &str,
        shard_id: &str,
    ) -> crate::Result<Option<CheckpointItem>> {
        let result = self
            .svc
            .get_item()
            .table_name(&self.conf.table)
            .key("StreamID", AttributeValue::S(stream_id.to_string()))
            .key("ShardID", AttributeValue::S(shard_id.to_string()))
            .send()
            .await
            .map_err(|e| format!("DynamoDB GetItem error: {}", DisplayErrorContext(&e)))?;

        let item = match result.item {
            Some(i) => i,
            None => return Ok(None),
        };

        let sequence = match item.get("SequenceNumber") {
            Some(AttributeValue::S(s)) => s.clone(),
            _ => return Err("SequenceNumber not found in checkpoint item".into()),
        };

        let client_id = match item.get("ClientID") {
            Some(AttributeValue::S(s)) => Some(s.clone()),
            _ => None,
        };

        let lease_timeout = match item.get("LeaseTimeout") {
            Some(AttributeValue::S(s)) => {
                let dt = s
                    .parse::<DateTime<Utc>>()
                    .map_err(|e| format!("Failed to parse LeaseTimeout: {e}"))?;
                Some(dt)
            }
            _ => None,
        };

        Ok(Some(CheckpointItem {
            sequence_number: sequence,
            client_id,
            lease_timeout,
        }))
    }

    /// Query all checkpoint items for a stream in a single (paginated) DynamoDB Query.
    pub async fn get_checkpoints_and_claims(
        &self,
        stream_id: &str,
    ) -> crate::Result<CheckpointData> {
        let mut data = CheckpointData {
            shards_with_checkpoints: HashMap::new(),
            sequence_numbers: HashMap::new(),
            client_claims: HashMap::new(),
        };

        let mut last_key: Option<HashMap<String, AttributeValue>> = None;

        loop {
            let mut req = self
                .svc
                .query()
                .table_name(&self.conf.table)
                .key_condition_expression("StreamID = :stream_id")
                .expression_attribute_values(
                    ":stream_id",
                    AttributeValue::S(stream_id.to_string()),
                );

            if let Some(ref key) = last_key {
                for (k, v) in key {
                    req = req.exclusive_start_key(k, v.clone());
                }
            }

            let result = req
                .send()
                .await
                .map_err(|e| format!("DynamoDB Query error: {}", DisplayErrorContext(&e)))?;

            for item in result.items.unwrap_or_default() {
                let shard_id = match item.get("ShardID") {
                    Some(AttributeValue::S(s)) => s.clone(),
                    _ => continue, // skip malformed rows
                };

                data.shards_with_checkpoints.insert(shard_id.clone(), true);

                let sequence_number = match item.get("SequenceNumber") {
                    Some(AttributeValue::S(s)) => s.clone(),
                    _ => String::new(),
                };
                data.sequence_numbers
                    .insert(shard_id.clone(), sequence_number);

                let client_id = match item.get("ClientID") {
                    Some(AttributeValue::S(s)) => s.clone(),
                    _ => continue, // no owner, orphaned checkpoint
                };

                let lease_timeout = match item.get("LeaseTimeout") {
                    Some(AttributeValue::S(s)) => match s.parse::<DateTime<Utc>>() {
                        Ok(dt) => dt,
                        Err(e) => {
                            return Err(format!(
                                "Failed to parse LeaseTimeout for shard {shard_id}: {e}"
                            )
                            .into());
                        }
                    },
                    _ => {
                        return Err(
                            format!("Missing LeaseTimeout for claimed shard {shard_id}").into()
                        );
                    }
                };

                data.client_claims
                    .entry(client_id)
                    .or_default()
                    .push(ClientClaim {
                        shard_id,
                        lease_timeout,
                    });
            }

            match result.last_evaluated_key {
                Some(key) if !key.is_empty() => last_key = Some(key),
                _ => break,
            }
        }

        Ok(data)
    }

    /// Attempt to claim a shard. If `from_client_id` is non-empty, steal from that client.
    /// Returns the last known sequence number on success.
    pub async fn claim(
        &self,
        stream_id: &str,
        shard_id: &str,
        from_client_id: &str,
    ) -> Result<String, CheckpointerError> {
        let new_lease_timeout = format_lease_timeout(self.lease_duration);

        let (condition_expr, mut attr_values): (&str, HashMap<String, AttributeValue>) =
            if !from_client_id.is_empty() {
                let mut v = HashMap::new();
                v.insert(
                    ":new_client_id".to_string(),
                    AttributeValue::S(self.client_id.clone()),
                );
                v.insert(
                    ":new_lease_timeout".to_string(),
                    AttributeValue::S(new_lease_timeout.clone()),
                );
                v.insert(
                    ":old_client_id".to_string(),
                    AttributeValue::S(from_client_id.to_string()),
                );
                ("ClientID = :old_client_id", v)
            } else {
                let mut v = HashMap::new();
                v.insert(
                    ":new_client_id".to_string(),
                    AttributeValue::S(self.client_id.clone()),
                );
                v.insert(
                    ":new_lease_timeout".to_string(),
                    AttributeValue::S(new_lease_timeout.clone()),
                );
                ("attribute_not_exists(ClientID)", v)
            };

        let mut req = self
            .svc
            .update_item()
            .table_name(&self.conf.table)
            .key("StreamID", AttributeValue::S(stream_id.to_string()))
            .key("ShardID", AttributeValue::S(shard_id.to_string()))
            .condition_expression(condition_expr)
            .update_expression("SET ClientID = :new_client_id, LeaseTimeout = :new_lease_timeout")
            .return_values(ReturnValue::AllOld);

        for (k, v) in attr_values.drain() {
            req = req.expression_attribute_values(k, v);
        }

        let result = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                let is_condition_failed = e
                    .as_service_error()
                    .map(|se| se.is_conditional_check_failed_exception())
                    .unwrap_or(false);
                if is_condition_failed {
                    return Err(CheckpointerError::LeaseNotAcquired);
                }
                return Err(CheckpointerError::Other(
                    format!("DynamoDB UpdateItem error: {}", DisplayErrorContext(&e)).into(),
                ));
            }
        };

        let attrs = result.attributes.unwrap_or_default();

        let mut starting_sequence = match attrs.get("SequenceNumber") {
            Some(AttributeValue::S(s)) => s.clone(),
            _ => String::new(),
        };

        // If we stole an active lease, wait a grace period so the previous owner
        // can flush its final sequence number before we start consuming.
        if !from_client_id.is_empty() {
            let old_lease: Option<DateTime<Utc>> = match attrs.get("LeaseTimeout") {
                Some(AttributeValue::S(s)) => s.parse::<DateTime<Utc>>().ok(),
                _ => None,
            };

            if let Some(old_timeout) = old_lease {
                let remaining = old_timeout
                    .signed_duration_since(Utc::now())
                    .to_std()
                    .unwrap_or(Duration::ZERO);

                if !remaining.is_zero() {
                    let wait_for = remaining + Duration::from_secs(1);
                    tokio::time::sleep(wait_for).await;

                    if let Ok(Some(cp)) = self.get_checkpoint(stream_id, shard_id).await {
                        starting_sequence = cp.sequence_number;
                    }
                }
            }
        }

        Ok(starting_sequence)
    }

    /// Persist the latest acknowledged sequence number. If `final_checkpoint` is true,
    /// the ClientID and LeaseTimeout are omitted, releasing the lease.
    ///
    /// Returns `true` if this client still owns the shard, `false` if ownership was lost.
    pub async fn checkpoint(
        &self,
        stream_id: &str,
        shard_id: &str,
        sequence_number: &str,
        final_checkpoint: bool,
    ) -> crate::Result<bool> {
        let mut item: HashMap<String, AttributeValue> = [
            (
                "StreamID".to_string(),
                AttributeValue::S(stream_id.to_string()),
            ),
            (
                "ShardID".to_string(),
                AttributeValue::S(shard_id.to_string()),
            ),
        ]
        .into_iter()
        .collect();

        if !sequence_number.is_empty() {
            item.insert(
                "SequenceNumber".to_string(),
                AttributeValue::S(sequence_number.to_string()),
            );
        }

        if !final_checkpoint {
            item.insert(
                "ClientID".to_string(),
                AttributeValue::S(self.client_id.clone()),
            );
            item.insert(
                "LeaseTimeout".to_string(),
                AttributeValue::S(format_lease_timeout(self.lease_duration)),
            );
        }

        let result = self
            .svc
            .put_item()
            .table_name(&self.conf.table)
            .set_item(Some(item))
            .condition_expression("ClientID = :client_id")
            .expression_attribute_values(":client_id", AttributeValue::S(self.client_id.clone()))
            .send()
            .await;

        match result {
            Ok(_) => Ok(true),
            Err(e) => {
                let is_condition_failed = e
                    .as_service_error()
                    .map(|se| se.is_conditional_check_failed_exception())
                    .unwrap_or(false);
                if is_condition_failed {
                    return Ok(false);
                }
                Err(format!("DynamoDB PutItem error: {}", DisplayErrorContext(&e)).into())
            }
        }
    }

    /// Update only the `SequenceNumber` field of an existing checkpoint row.
    /// Used when yielding a shard to allow the new owner to start from the latest
    /// sequence rather than the sequence at the time of the steal.
    pub async fn yield_shard(
        &self,
        stream_id: &str,
        shard_id: &str,
        sequence_number: &str,
    ) -> crate::Result<()> {
        if sequence_number.is_empty() {
            return Ok(());
        }

        self.svc
            .update_item()
            .table_name(&self.conf.table)
            .key("StreamID", AttributeValue::S(stream_id.to_string()))
            .key("ShardID", AttributeValue::S(shard_id.to_string()))
            .update_expression("SET SequenceNumber = :new_sequence_number")
            .expression_attribute_values(
                ":new_sequence_number",
                AttributeValue::S(sequence_number.to_string()),
            )
            .send()
            .await
            .map_err(|e| format!("DynamoDB UpdateItem error: {}", DisplayErrorContext(&e)))?;
        Ok(())
    }

    /// Delete a checkpoint row entirely. Called when a shard has been fully consumed
    /// (its iterator returns null).
    pub async fn delete(&self, stream_id: &str, shard_id: &str) -> crate::Result<()> {
        self.svc
            .delete_item()
            .table_name(&self.conf.table)
            .key("StreamID", AttributeValue::S(stream_id.to_string()))
            .key("ShardID", AttributeValue::S(shard_id.to_string()))
            .send()
            .await
            .map_err(|e| format!("DynamoDB DeleteItem error: {}", DisplayErrorContext(&e)))?;
        Ok(())
    }
}

fn format_lease_timeout(duration: Duration) -> String {
    let expires =
        Utc::now() + chrono::Duration::from_std(duration).unwrap_or(chrono::Duration::zero());
    expires.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
}
