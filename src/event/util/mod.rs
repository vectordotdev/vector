#![allow(deprecated)]

pub mod log;

pub(self) use super::{LogEvent, Value};
use bytes::Bytes;
use serde::de::{MapAccess, SeqAccess, Visitor};
use std::collections::HashMap;
use std::fmt::{self};

