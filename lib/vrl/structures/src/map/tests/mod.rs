use crate::map::Map;
use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
enum Operation {
    Insert(u16, u8),
    Remove(u16),
    Get(u16),
    GetMut(u16),
    GetLen,
    GetIsEmpty,
    Clear,
    ContainsKey(u16),
}

impl Arbitrary for Operation {
    fn arbitrary(gen: &mut Gen) -> Self {
        let variant: u8 = u8::arbitrary(gen);
        match variant % 8 {
            0 => Operation::Insert(u16::arbitrary(gen), u8::arbitrary(gen)),
            1 => Operation::Remove(u16::arbitrary(gen)),
            2 => Operation::Get(u16::arbitrary(gen)),
            3 => Operation::GetLen,
            4 => Operation::GetIsEmpty,
            5 => Operation::Clear,
            6 => Operation::ContainsKey(u16::arbitrary(gen)),
            7 => Operation::GetMut(u16::arbitrary(gen)),
            _ => unreachable!(),
        }
    }
}

#[test]
fn model_check() {
    fn inner(input: Vec<Operation>) -> TestResult {
        let mut model: BTreeMap<u16, u8> = BTreeMap::new();
        let mut sut: Map<u16, u8> = Map::new();

        for op in &input {
            match op {
                Operation::Insert(k, v) => assert_eq!(model.insert(*k, *v), sut.insert(*k, *v)),
                Operation::Remove(k) => assert_eq!(model.remove(k), sut.remove(k)),
                Operation::Get(k) => assert_eq!(model.get(k), sut.get(k)),
                Operation::GetLen => assert_eq!(model.len(), sut.len()),
                Operation::GetIsEmpty => assert_eq!(model.is_empty(), sut.is_empty()),
                Operation::Clear => assert_eq!(model.clear(), sut.clear()),
                Operation::ContainsKey(k) => assert_eq!(model.contains_key(k), sut.contains_key(k)),
                Operation::GetMut(k) => assert_eq!(model.get_mut(k), sut.get_mut(k)),
            }
        }

        TestResult::passed()
    }
    QuickCheck::new()
        .max_tests(1_000)
        .tests(500)
        .quickcheck(inner as fn(Vec<Operation>) -> TestResult);
}
