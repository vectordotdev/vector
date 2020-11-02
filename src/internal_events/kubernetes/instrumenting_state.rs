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

#[derive(Debug)]
pub struct StateMaintenanceRequested;

#[derive(Debug)]
pub struct StateMaintenancePerformed;

enum OpKind {
    ItemAdded,
    ItemDeleted,
    ItemUpdated,
    MaintenancePerformed,
    MaintenanceRequested,
    Resynced,
}

impl OpKind {
    fn as_str(&self) -> &str {
        match self {
            Self::ItemAdded => "item_added",
            Self::ItemDeleted => "item_deleted",
            Self::ItemUpdated => "item_updated",
            Self::MaintenancePerformed => "maintenance_performed",
            Self::MaintenanceRequested => "maintenance_requested",
            Self::Resynced => "resynced",
        }
    }
}

impl InternalEvent for StateItemAdded {
    fn emit_metrics(&self) {
        counter!("k8s_state_ops_total", 1, "op_kind" => OpKind::ItemAdded.as_str());
    }
}

impl InternalEvent for StateItemUpdated {
    fn emit_metrics(&self) {
        counter!("k8s_state_ops_total", 1, "op_kind" => OpKind::ItemUpdated.as_str());
    }
}

impl InternalEvent for StateItemDeleted {
    fn emit_metrics(&self) {
        counter!("k8s_state_ops_total", 1, "op_kind" => OpKind::ItemDeleted.as_str());
    }
}

impl InternalEvent for StateResynced {
    fn emit_metrics(&self) {
        counter!("k8s_state_ops_total", 1, "op_kind" => OpKind::Resynced.as_str());
    }
}

impl InternalEvent for StateMaintenanceRequested {
    fn emit_metrics(&self) {
        counter!("k8s_state_ops_total", 1, "op_kind" => OpKind::MaintenanceRequested.as_str());
    }
}

impl InternalEvent for StateMaintenancePerformed {
    fn emit_metrics(&self) {
        counter!("k8s_state_ops_total", 1, "op_kind" => OpKind::MaintenancePerformed.as_str());
    }
}
