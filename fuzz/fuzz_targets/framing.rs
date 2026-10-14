#![no_main]
use libfuzzer_sys::fuzz_target;
use tell_tail::framing::Framer;

fuzz_target!(|data: &[u8]| {
    if data.len() < 3 {
        return;
    }
    let limit = usize::from(u16::from_le_bytes([data[0], data[1]])) + 1;
    let chunk = usize::from(data[2]) + 1;
    let input = &data[3..];
    let mut reference: Vec<_> = input.split(|&b| b == b'\n').collect();
    if reference.last() == Some(&&b""[..]) {
        reference.pop();
    }
    let expected: Vec<_> = reference
        .into_iter()
        .map(|line| {
            if line.len() > limit {
                (vec![], true)
            } else {
                (line.strip_suffix(b"\r").unwrap_or(line).to_vec(), false)
            }
        })
        .collect();
    let mut framer = Framer::new(limit);
    let mut actual = vec![];
    for part in input.chunks(chunk) {
        framer.feed(part, |line, oversized| {
            actual.push((line.to_vec(), oversized))
        });
    }
    framer.finish(|line, oversized| actual.push((line.to_vec(), oversized)));
    framer.finish(|_, _| panic!("duplicate EOF"));
    assert_eq!(actual, expected);
    assert_eq!(framer.pending_bytes(), 0);
});
