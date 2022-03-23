use std::fmt::Debug;

use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct StateItemAdded;

#[derive(Debug)]
pub struct StateItemUpdated;

#[derive(Debug)]
pub struct StateItemDeleted;

#[derive(Debug)]
pub struct StateResynced;

#[derive(Debug)]
pub struct StateMaintenanceRequested;

#[derive(Debug)]
pub struct StateMaintenancePerformed;

impl InternalEvent for StateItemAdded {
    fn emit(self) {
        counter!("k8s_state_ops_total", 1, "op_kind" => "item_added");
    }
}

impl InternalEvent for StateItemUpdated {
    fn emit(self) {
        counter!("k8s_state_ops_total", 1, "op_kind" => "item_updated");
    }
}

impl InternalEvent for StateItemDeleted {
    fn emit(self) {
        counter!("k8s_state_ops_total", 1, "op_kind" => "item_deleted");
    }
}

impl InternalEvent for StateResynced {
    fn emit(self) {
        counter!("k8s_state_ops_total", 1, "op_kind" => "resynced");
    }
}

impl InternalEvent for StateMaintenanceRequested {
    fn emit(self) {
        counter!("k8s_state_ops_total", 1, "op_kind" => "maintenance_requested");
    }
}

impl InternalEvent for StateMaintenancePerformed {
    fn emit(self) {
        counter!("k8s_state_ops_total", 1, "op_kind" => "maintenance_performed");
    }
}
