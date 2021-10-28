#![cfg(test)]

use super::*;
use crate::{event::Event, test_util::random_string};
use std::collections::BTreeMap;

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<KinesisSinkConfig>();
}
