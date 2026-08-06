use crate::Template;

#[derive(Debug, Clone)]
pub struct RenderPlan {
    head: Vec<u8>,
    segments: Vec<RenderSegment>,
    max_size: usize,
}

#[derive(Debug, Clone)]
struct RenderSegment {
    arg_idx: usize,
    tail: Vec<u8>,
}

impl RenderPlan {
    pub fn new(t: &Template, max_arg_len: Option<&dyn Fn(usize) -> usize>) -> Self {
        let mut head: Vec<u8> = Vec::new();
        let mut segments: Vec<RenderSegment> = Vec::new();
        let mut arg_idx = 0usize;
        let mut tok_idx = 0usize;
        let mut cur: Vec<u8> = Vec::new();

        for i in 0..t.token_count() {
            if i > 0 {
                cur.push(b' ');
            }
            if t.is_param(i) {
                if let Some(last) = segments.last_mut() {
                    last.tail = cur;
                } else {
                    head = cur;
                }
                segments.push(RenderSegment {
                    arg_idx,
                    tail: Vec::new(),
                });
                cur = Vec::new();
                arg_idx += 1;
            } else {
                cur.extend_from_slice(t.tokens()[tok_idx].as_bytes());
                tok_idx += 1;
            }
        }
        if let Some(last) = segments.last_mut() {
            last.tail = cur;
        } else {
            head = cur;
        }

        let mut max_size = head.len();
        for seg in &segments {
            max_size += seg.tail.len();
            if let Some(f) = max_arg_len {
                max_size += f(seg.arg_idx);
            }
        }
        Self {
            head,
            segments,
            max_size,
        }
    }

    pub fn max_size(&self) -> usize {
        self.max_size
    }

    pub fn append(&self, dst: &mut Vec<u8>, args: Option<&[&str]>) {
        dst.extend_from_slice(&self.head);
        for seg in &self.segments {
            if let Some(s) = args.and_then(|a| a.get(seg.arg_idx)) {
                dst.extend_from_slice(s.as_bytes());
            }
            dst.extend_from_slice(&seg.tail);
        }
    }
}
