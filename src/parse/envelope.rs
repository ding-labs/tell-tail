use super::text::iso_timestamp;
use crate::cli::Envelope;
use serde::Deserialize;
use std::{borrow::Cow, cell::Cell};

pub(super) struct Decoder {
    kind: Envelope,
    partial: Cell<[bool; 2]>,
}
impl Decoder {
    pub fn new(kind: Envelope) -> Self {
        Self {
            kind,
            partial: Cell::new([false; 2]),
        }
    }
    pub fn reset(&self) {
        self.partial.set([false; 2]);
    }
    pub fn lost_record(&self) {
        self.partial.set([true; 2]);
    }
    pub fn decode<'a>(&self, line: &'a [u8]) -> Result<Option<Cow<'a, [u8]>>, ()> {
        match self.kind {
            Envelope::Docker => docker(line),
            Envelope::Cri => {
                let result = self.cri(line);
                if result.is_err() {
                    self.lost_record();
                }
                result
            }
        }
    }
    fn cri<'a>(&self, line: &'a [u8]) -> Result<Option<Cow<'a, [u8]>>, ()> {
        let mut parts = line.splitn(4, |b| *b == b' ');
        let timestamp = parts.next().ok_or(())?;
        if !iso_timestamp(timestamp) || timestamp.get(10) != Some(&b'T') {
            return Err(());
        }
        let stream = match parts.next().ok_or(())? {
            b"stdout" => 0,
            b"stderr" => 1,
            _ => return Err(()),
        };
        let tag = parts.next().ok_or(())?;
        let payload = parts.next().ok_or(())?;
        let mut partial = self.partial.get();
        match tag {
            b"P" => {
                partial[stream] = true;
                self.partial.set(partial);
                Ok(None)
            }
            b"F" => {
                let fragmented = partial[stream];
                partial[stream] = false;
                self.partial.set(partial);
                Ok((!fragmented).then_some(Cow::Borrowed(payload)))
            }
            _ => Err(()),
        }
    }
}
#[derive(Deserialize)]
struct DockerRecord<'a> {
    #[serde(borrow)]
    log: Cow<'a, str>,
    #[serde(borrow)]
    stream: Cow<'a, str>,
    #[serde(borrow)]
    time: Cow<'a, str>,
}
fn docker(line: &[u8]) -> Result<Option<Cow<'_, [u8]>>, ()> {
    if !super::valid_unicode_escapes(line) {
        return Err(());
    }
    let record: DockerRecord<'_> = serde_json::from_slice(line).map_err(|_| ())?;
    if !matches!(record.stream.as_ref(), "stdout" | "stderr")
        || !iso_timestamp(record.time.as_bytes())
    {
        return Err(());
    }
    // Preserve one physical envelope = one record; do not invent inner event counts.
    let content = record.log.strip_suffix('\n').unwrap_or(&record.log);
    let content = content.strip_suffix('\r').unwrap_or(content);
    if content.contains('\n') || content.contains('\r') {
        return Ok(None);
    }
    let len = content.len();
    Ok(Some(match record.log {
        Cow::Borrowed(value) => Cow::Borrowed(&value.as_bytes()[..len]),
        Cow::Owned(mut value) => {
            value.truncate(len);
            Cow::Owned(value.into_bytes())
        }
    }))
}
