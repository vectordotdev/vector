use arbitrary::Unstructured;

pub(crate) trait ArbitraryDepth<'a>: Sized {
    fn arbitrary_depth(u: &mut Unstructured<'a>, depth: isize) -> arbitrary::Result<Self>;
}
