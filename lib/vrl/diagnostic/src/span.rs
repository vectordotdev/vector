/// A region of code in a source file
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct Span {
    start: usize,
    end: usize,
}

impl Span {
    #[must_use]
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Get the start index
    #[must_use]
    pub fn start(self) -> usize {
        self.start
    }

    /// Get the end index
    #[must_use]
    pub fn end(self) -> usize {
        self.end
    }

    #[must_use]
    pub fn range(self) -> std::ops::Range<usize> {
        self.start..self.end
    }
}

impl std::ops::Add<usize> for Span {
    type Output = Self;

    fn add(self, other: usize) -> Self {
        Self {
            start: self.start + other,
            end: self.end + other,
        }
    }
}

impl From<&Span> for Span {
    fn from(span: &Span) -> Self {
        *span
    }
}

impl From<(usize, usize)> for Span {
    fn from((start, end): (usize, usize)) -> Self {
        Self { start, end }
    }
}

#[must_use]
pub fn span(start: usize, end: usize) -> Span {
    Span { start, end }
}
