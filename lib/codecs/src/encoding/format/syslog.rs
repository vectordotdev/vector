use bytes::{BufMut, BytesMut};
use tokio_util::codec::Encoder;
use vector_core::{config::DataType, event::{Event, LogEvent}, schema};
use chrono::{DateTime, SecondsFormat, Local};
use vrl::value::Value;
use serde::{de, Deserialize};
use vector_config::configurable_component;
