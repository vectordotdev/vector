///! Contains backwards compatibility with lookup "v1"
///! This is all temporary and will be deleted when migration to the V2 lookup code is complete.
use crate::lookup_v2::{BorrowedSegment, OwnedPath, OwnedSegment, Path};
use crate::{FieldBuf, LookupBuf, SegmentBuf};
use std::borrow::Cow;

impl<'a> Path<'a> for &'a LookupBuf {
    type Iter = LookupBufPathIter<'a>;

    fn segment_iter(&self) -> Self::Iter {
        LookupBufPathIter {
            buf: self,
            segment_i: 0,
            coalesce_i: 0,
        }
    }
}

impl From<LookupBuf> for OwnedPath {
    fn from(lookup: LookupBuf) -> Self {
        let segments = lookup
            .segments
            .into_iter()
            .map(|segment| match segment {
                SegmentBuf::Field(field) => OwnedSegment::Field(field.name),
                SegmentBuf::Index(i) => OwnedSegment::Index(i),
                SegmentBuf::Coalesce(fields) => {
                    let fields = fields.into_iter().map(|field| field.name).collect();
                    OwnedSegment::Coalesce(fields)
                }
            })
            .collect();
        Self { segments }
    }
}

// This should only be used if the `OwnedPath` has already been verified to be valid.
impl From<OwnedPath> for LookupBuf {
    fn from(path: OwnedPath) -> Self {
        let segments = path
            .segments
            .into_iter()
            .map(|segment| match segment {
                OwnedSegment::Field(field) => SegmentBuf::Field(FieldBuf::from(field)),
                OwnedSegment::Index(i) => SegmentBuf::Index(i),
                OwnedSegment::Coalesce(fields) => {
                    let fields = fields.into_iter().map(FieldBuf::from).collect();
                    SegmentBuf::Coalesce(fields)
                }
                OwnedSegment::Invalid => {
                    // eventually "Invalid" will be removed from `OwnedPath`, but until then
                    // this compatibility layer should only be used where OwnedPath can never be Invalid
                    // (such as after being converted directly from a LookupBuf)
                    unreachable!(
                        "compatibility layer shouldn't be used if OwnedPath can be invalid!"
                    )
                }
            })
            .collect();
        LookupBuf::from_segments(segments)
    }
}

#[derive(Clone)]
pub struct LookupBufPathIter<'a> {
    buf: &'a LookupBuf,
    segment_i: usize,
    coalesce_i: usize,
}

impl<'a> Iterator for LookupBufPathIter<'a> {
    type Item = BorrowedSegment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.buf
            .segments
            .get(self.segment_i)
            .map(|segment| match segment {
                SegmentBuf::Field(field) => {
                    self.segment_i += 1;
                    BorrowedSegment::Field(Cow::Borrowed(&field.name))
                }
                SegmentBuf::Index(index) => {
                    self.segment_i += 1;
                    BorrowedSegment::Index(*index)
                }
                SegmentBuf::Coalesce(fields) => {
                    let field = fields
                        .get(self.coalesce_i)
                        .expect("coalesce fields must not be empty");
                    if self.coalesce_i == fields.len() - 1 {
                        self.coalesce_i = 0;
                        self.segment_i += 1;
                        BorrowedSegment::CoalesceEnd(Cow::Borrowed(&field.name))
                    } else {
                        self.coalesce_i += 1;
                        BorrowedSegment::CoalesceField(Cow::Borrowed(&field.name))
                    }
                }
            })
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
            ".(a|b|c).foo.bar[42]",
        ];

        for test in tests {
            let lookup_buf = LookupBuf::from_str(test).unwrap();
            if !Path::eq(&test, &lookup_buf) {
                panic!("Equality failed. Path={:?}", test);
            }
        }
    }
}
