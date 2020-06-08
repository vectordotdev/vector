use super::InternalEvent;
use metrics::counter;
use std::fmt::Debug;

#[derive(Debug)]
pub struct StateItemAdded;

#[derive(Debug)]
pub struct StateItemUpdated;

#[derive(Debug)]
pub struct StateItemDeleted;

#[derive(Debug)]
pub struct StateResynced;

impl InternalEvent for StateItemAdded {
    fn emit_metrics(&self) {
        counter!("k8s_state_ops", 1, "op_kind" => "item_added");
    }
}

impl InternalEvent for StateItemUpdated {
    fn emit_metrics(&self) {
        counter!("k8s_state_ops", 1, "op_kind" => "item_updated");
    }
}

impl InternalEvent for StateItemDeleted {
    fn emit_metrics(&self) {
        counter!("k8s_state_ops", 1, "op_kind" => "item_deleted");
    }
}

impl InternalEvent for StateResynced {
    fn emit_metrics(&self) {
        counter!("k8s_state_ops", 1, "op_kind" => "resynced");
    }
}
