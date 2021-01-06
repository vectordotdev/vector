use super::InternalEvent;
use metrics::{counter, gauge};
use rdkafka::Statistics;

#[derive(Debug)]
pub struct KafkaEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for KafkaEventReceived {
    fn emit_logs(&self) {
        trace!(message = "Received one event.", internal_log_rate_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct KafkaOffsetUpdateFailed {
    pub error: rdkafka::error::KafkaError,
}

impl InternalEvent for KafkaOffsetUpdateFailed {
    fn emit_logs(&self) {
        error!(message = "Unable to update consumer offset.", error = ?self.error);
    }

    fn emit_metrics(&self) {
        counter!("consumer_offset_updates_failed_total", 1);
    }
}

#[derive(Debug)]
pub struct KafkaEventFailed {
    pub error: rdkafka::error::KafkaError,
}

impl InternalEvent for KafkaEventFailed {
    fn emit_logs(&self) {
        error!(message = "Failed to read message.", error = ?self.error);
    }

    fn emit_metrics(&self) {
        counter!("events_failed_total", 1);
    }
}

#[derive(Debug)]
pub struct KafkaKeyExtractionFailed<'a> {
    pub key_field: &'a str,
}

impl InternalEvent for KafkaKeyExtractionFailed<'_> {
    fn emit_logs(&self) {
        error!(message = "Failed to extract key.", key_field = %self.key_field);
    }
}

#[derive(Debug)]
pub struct KafkaStatistics<'a> {
    statistics: &'a Statistics,
}

impl<'a> KafkaStatistics<'a> {
    pub(crate) fn new(statistics: &'a Statistics) -> Self {
        Self { statistics }
    }
}

impl InternalEvent for KafkaStatistics<'_> {
    fn emit_metrics(&self) {
        // gauge!("name", self.statistics.name);
        // gauge!("client_id", self.statistics.client_id);
        // gauge!("client_type", self.statistics.client_type);
        gauge!("ts", self.statistics.ts as f64);
        gauge!("time", self.statistics.time as f64);
        gauge!("replyq", self.statistics.replyq as f64);
        gauge!("msg_cnt", self.statistics.msg_cnt as f64);
        gauge!("msg_size", self.statistics.msg_size as f64);
        gauge!("msg_max", self.statistics.msg_max as f64);
        gauge!("msg_size_max", self.statistics.msg_size_max as f64);
        gauge!("tx", self.statistics.tx as f64);
        gauge!("tx_bytes", self.statistics.tx_bytes as f64);
        gauge!("rx", self.statistics.rx as f64);
        gauge!("rx_bytes", self.statistics.rx_bytes as f64);
        gauge!("txmsgs", self.statistics.txmsgs as f64);
        gauge!("txmsg_bytes", self.statistics.txmsg_bytes as f64);
        gauge!("rxmsgs", self.statistics.rxmsgs as f64);
        gauge!("rxmsg_bytes", self.statistics.rxmsg_bytes as f64);
        gauge!("simple_cnt", self.statistics.simple_cnt as f64);
        gauge!(
            "metadata_cache_cnt",
            self.statistics.metadata_cache_cnt as f64
        );

        for (name, broker) in &self.statistics.brokers {
            let labels_broker = [("broker", name.clone())];

            // gauge!("broker_name", broker.name, &labels_broker);
            // gauge!("broker_nodeid", broker.nodeid, &labels_broker);
            // gauge!("broker_nodename", broker.nodename, &labels_broker);
            // gauge!("broker_source", broker.source, &labels_broker);
            // gauge!("broker_state", broker.state, &labels_broker);
            gauge!("broker_stateage", broker.stateage as f64, &labels_broker);
            gauge!(
                "broker_outbuf_cnt",
                broker.outbuf_cnt as f64,
                &labels_broker
            );
            gauge!(
                "broker_outbuf_msg_cnt",
                broker.outbuf_msg_cnt as f64,
                &labels_broker
            );
            gauge!(
                "broker_waitresp_cnt",
                broker.waitresp_cnt as f64,
                &labels_broker
            );
            gauge!(
                "broker_waitresp_msg_cnt",
                broker.waitresp_msg_cnt as f64,
                &labels_broker
            );
            gauge!("broker_tx", broker.tx as f64, &labels_broker);
            gauge!("broker_txbytes", broker.txbytes as f64, &labels_broker);
            gauge!("broker_txerrs", broker.txerrs as f64, &labels_broker);
            gauge!("broker_txretries", broker.txretries as f64, &labels_broker);
            gauge!(
                "broker_req_timeouts",
                broker.req_timeouts as f64,
                &labels_broker
            );
            gauge!("broker_rx", broker.rx as f64, &labels_broker);
            gauge!("broker_rxbytes", broker.rxbytes as f64, &labels_broker);
            gauge!("broker_rxerrs", broker.rxerrs as f64, &labels_broker);
            gauge!(
                "broker_rxcorriderrs",
                broker.rxcorriderrs as f64,
                &labels_broker
            );
            gauge!("broker_rxpartial", broker.rxpartial as f64, &labels_broker);
            gauge!("broker_zbuf_grow", broker.zbuf_grow as f64, &labels_broker);
            gauge!("broker_buf_grow", broker.buf_grow as f64, &labels_broker);
            if let Some(wakeups) = broker.wakeups {
                gauge!("broker_wakeups", wakeups as f64, &labels_broker);
            }
            if let Some(connects) = broker.connects {
                gauge!("broker_connects", connects as f64, &labels_broker);
            }
            if let Some(disconnects) = broker.disconnects {
                gauge!("broker_disconnects", disconnects as f64, &labels_broker);
            }
            // gauge!("broker_int_latency", broker.int_latency, &labels_broker);
            // gauge!("broker_outbuf_latency", broker.outbuf_latency, &labels_broker);
            // gauge!("broker_rtt", broker.rtt, &labels_broker);
            // gauge!("broker_throttle", broker.throttle, &labels_broker);
            // gauge!("broker_toppars", broker.toppars, &labels_broker);
        }

        for (name, topic) in &self.statistics.topics {
            let labels_topic = [("topic", name.clone())];

            // gauge!("topic_topic", topic.topic as f64, &labels_topic);
            gauge!(
                "topic_metadata_age",
                topic.metadata_age as f64,
                &labels_topic
            );
            // gauge!("topic_batchsize", topic.batchsize, &labels_topic);
            // gauge!("topic_batchcnt", topic.batchcnt, &labels_topic);
            // gauge!("topic_partitions", topic.partitions, &labels_topic);
        }

        if let Some(cgrp) = &self.statistics.cgrp {
            // gauge!("cgrp_state", cgrp.state);
            gauge!("cgrp_stateage", cgrp.stateage as f64);
            // gauge!("cgrp_join_state", cgrp.join_state);
            gauge!("cgrp_rebalance_age", cgrp.rebalance_age as f64);
            gauge!("cgrp_rebalance_cnt", cgrp.rebalance_cnt as f64);
            // gauge!("cgrp_rebalance_reason", cgrp.rebalance_reason);
            gauge!("cgrp_assignment_size", cgrp.assignment_size as f64);
        }

        if let Some(eos) = &self.statistics.eos {
            // gauge!("eos_idemp_state", eos.idemp_state);
            gauge!("eos_idemp_stateage", eos.idemp_stateage as f64);
            // gauge!("eos_txn_state", eos.txn_state);
            gauge!("eos_txn_stateage", eos.txn_stateage as f64);
            gauge!("eos_txn_may_enq", eos.txn_may_enq as u32 as f64);
            gauge!("eos_producer_id", eos.producer_id as f64);
            gauge!("eos_producer_epoch", eos.producer_epoch as f64);
            gauge!("eos_epoch_cnt", eos.epoch_cnt as f64);
        }
    }
}
