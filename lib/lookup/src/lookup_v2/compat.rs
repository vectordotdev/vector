///! Contains backwards compatibility with lookup "v1"

use crate::lookup_v2::{BorrowedSegment, Path};
use crate::{LookupBuf, SegmentBuf};
use std::borrow::Cow;


impl <'a> Path<'a> for &'a LookupBuf {
    type Iter = LookupBufPathIter<'a>;

    fn segment_iter(&self) -> Self::Iter {
        LookupBufPathIter {
            buf: self,
            segment_i: 0,
            coalesce_i: 0,
        }
    }
}

pub struct LookupBufPathIter<'a> {
    buf: &'a LookupBuf,
    segment_i: usize,
    coalesce_i: usize,
}

impl <'a> Iterator for LookupBufPathIter<'a> {
    type Item = BorrowedSegment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.buf.segments.get(self.segment_i) {
            Some(SegmentBuf::Field(field)) => {
                self.segment_i += 1;
                Some(BorrowedSegment::Field(Cow::Borrowed(&field.name)))
            }
            Some(SegmentBuf::Index(index)) => {
                self.segment_i += 1;
                Some(BorrowedSegment::Index(*index))
            }
            Some(SegmentBuf::Coalesce(fields)) => {
                match fields.get(self.coalesce_i) {
                    Some(field) => {
                        if self.coalesce_i == fields.len() - 1 {
                            self.coalesce_i = 0;
                            self.segment_i += 1;
                            Some(BorrowedSegment::CoalesceEnd(Cow::Borrowed(&field.name)))
                        } else {
                            self.coalesce_i += 1;
                            Some(BorrowedSegment::CoalesceField(Cow::Borrowed(&field.name)))
                        }
                    },
                    None => unreachable!()
                }
            }
            None => None
        }
    }
}


#[cfg(test)]
mod test {
    use crate::lookup_v2::Path;
    use crate::LookupBuf;

    #[test]
    fn test() {
        let tests = [
            "foo.bar",
            "foo.bar[0]",
            ".",
            "[-5]",
            ".(a|b|c)",
            ".(a|b)",
            ".(a|b|c).foo.bar[42]"
        ];

        for test in tests {
            let lookup_buf = LookupBuf::from_str(test).unwrap();
            if !Path::eq(&test, &lookup_buf) {
                println!("Equality failed for {:?}", test);
                println!("V2: {:?}", test.segment_iter().collect::<Vec<_>>());
                println!("Compact: {:?}", (&lookup_buf).segment_iter().collect::<Vec<_>>());
                panic!("Equality failed");
            }
        }
    }
}
