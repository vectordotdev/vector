use crate::lookup_v2::BorrowedSegment;

/// When simplicity is preferred over performance (startup functions such as type definitions)
/// These segments can be easier to work with.

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SimpleSegment {
    Field(String),
    Index(isize),
    Coalesce(Vec<String>),
}

#[derive(Clone)]
pub struct SimpleSegmentIter<T> {
    iter: T,
}

impl<T> SimpleSegmentIter<T> {
    pub fn new(iter: T) -> Self {
        Self { iter }
    }
}

impl<'a, T: Iterator<Item = BorrowedSegment<'a>>> Iterator for SimpleSegmentIter<T> {
    type Item = SimpleSegment;

    fn next(&mut self) -> Option<Self::Item> {
        let mut coalesce_fields = vec![];

        loop {
            match self.iter.next() {
                Some(BorrowedSegment::Field(field)) => {
                    return Some(SimpleSegment::Field(field.to_string()))
                }
                Some(BorrowedSegment::Index(index)) => return Some(SimpleSegment::Index(index)),
                Some(BorrowedSegment::CoalesceField(field)) => {
                    coalesce_fields.push(field.to_string());
                }
                Some(BorrowedSegment::CoalesceEnd(field)) => {
                    coalesce_fields.push(field.to_string());
                    return Some(SimpleSegment::Coalesce(coalesce_fields));
                }
                Some(BorrowedSegment::Invalid) => {
                    unreachable!("already checked that invalid doesn't exist")
                }
                None => return None,
            }
        }
    }
}
