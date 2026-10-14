/// Frames physical lines without retaining more than `limit` bytes of a record.
/// Oversized lines produce exactly one `(empty, true)` event at their terminator.
pub struct Framer {
    partial: Vec<u8>,
    pending: u64,
    limit: usize,
    oversized: bool,
}
impl Framer {
    pub fn new(limit: usize) -> Self {
        assert!(limit > 0);
        Self {
            partial: Vec::new(),
            pending: 0,
            limit,
            oversized: false,
        }
    }
    pub fn pending_bytes(&self) -> u64 {
        self.pending
    }
    pub fn feed(&mut self, mut input: &[u8], mut emit: impl FnMut(&[u8], bool)) {
        while !input.is_empty() {
            let end = memchr::memchr(b'\n', input);
            let size = end.unwrap_or(input.len());
            let segment = &input[..size];
            self.pending = self.pending.saturating_add(size as u64);
            self.oversized |= self.pending > self.limit as u64;
            if end.is_some() {
                if self.oversized {
                    emit(&[], true);
                } else if self.partial.is_empty() {
                    emit(strip_cr(segment), false);
                } else {
                    self.partial.extend_from_slice(segment);
                    emit(strip_cr(&self.partial), false);
                }
                self.clear();
                input = &input[size + 1..];
            } else {
                if !self.oversized {
                    self.partial.extend_from_slice(segment);
                }
                break;
            }
        }
    }
    pub fn finish(&mut self, mut emit: impl FnMut(&[u8], bool)) {
        if self.pending != 0 {
            emit(
                if self.oversized {
                    &[]
                } else {
                    strip_cr(&self.partial)
                },
                self.oversized,
            );
            self.clear();
        }
    }
    /// Discard an incomplete record at a file generation boundary.
    pub fn clear(&mut self) {
        self.partial.clear();
        self.pending = 0;
        self.oversized = false;
    }
}
fn strip_cr(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn collect(input: &[u8], chunk: usize, limit: usize) -> Vec<(Vec<u8>, bool)> {
        let mut framer = Framer::new(limit);
        let mut result = vec![];
        for part in input.chunks(chunk) {
            framer.feed(part, |b, o| result.push((b.to_vec(), o)));
        }
        framer.finish(|b, o| result.push((b.to_vec(), o)));
        result
    }
    #[test]
    fn read_boundaries_do_not_change_records() {
        let input = b"INFO hi\r\n\n\xffERROR\nlast";
        let expected = vec![
            (b"INFO hi".to_vec(), false),
            (vec![], false),
            (b"\xffERROR".to_vec(), false),
            (b"last".to_vec(), false),
        ];
        for chunk in 1..=input.len() {
            assert_eq!(collect(input, chunk, 100), expected);
        }
        assert!(collect(b"", 1, 10).is_empty());
        assert_eq!(collect(b"a\n", 1, 10).len(), 1);
    }
    #[test]
    fn oversized_records_are_counted_once_and_recover() {
        for chunk in 1..20 {
            assert_eq!(
                collect(b"abcd\nabcde\nok\n0123456789", chunk, 4),
                vec![
                    (b"abcd".to_vec(), false),
                    (vec![], true),
                    (b"ok".to_vec(), false),
                    (vec![], true)
                ]
            );
        }
    }
    #[test]
    fn unterminated_input_memory_is_bounded() {
        let mut framer = Framer::new(1024);
        for _ in 0..10000 {
            framer.feed(&[b'x'; 256], |_, _| panic!("not a record yet"));
        }
        assert_eq!(framer.pending_bytes(), 2_560_000);
        assert!(framer.partial.len() <= 1024);
        assert!(framer.partial.capacity() <= 2048);
        let mut count = 0;
        framer.finish(|_, oversized| {
            assert!(oversized);
            count += 1;
        });
        framer.finish(|_, _| count += 1);
        assert_eq!(count, 1);
    }
}
