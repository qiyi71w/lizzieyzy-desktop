const MAX_FRAME: usize = 65_536;

#[derive(Default, Debug)]
pub struct Framer {
    pending: Vec<u8>,
}

impl Framer {
    /// Delivers each validated ASCII frame immediately, including before a later error.
    /// An error clears the partial frame and ignores the remaining bytes in this call.
    pub fn push(&mut self, bytes: &[u8], mut accept: impl FnMut(String)) -> Result<(), String> {
        for &byte in bytes {
            if !byte.is_ascii() || self.pending.len() >= MAX_FRAME {
                self.pending.clear();
                return Err("Invalid Fox ASCII frame".into());
            }
            self.pending.push(byte);
            if byte == b'\n' {
                let decoded = decode(&self.pending);
                self.pending.clear();
                accept(decoded?);
            }
        }
        Ok(())
    }
}

fn decode(frame: &[u8]) -> Result<String, String> {
    let n = frame.len();
    if n < 7 || frame[0] != b'$' || &frame[n - 2..] != b"\r\n" || frame[n - 5] != b'*' {
        return Err("Invalid Fox frame boundary".into());
    }
    let body = &frame[1..n - 5];
    let checksum = checksum(body)?;
    let digit = |b: u8| (b as char).to_digit(16).map(|v| v as u8);
    match (digit(frame[n - 4]), digit(frame[n - 3])) {
        (Some(hi), Some(lo)) if checksum == (hi << 4 | lo) => {
            Ok(String::from_utf8(body.to_vec()).expect("validated ASCII"))
        }
        _ => Err("Invalid Fox checksum".into()),
    }
}

fn checksum(body: &[u8]) -> Result<u8, String> {
    let mut xor = 0;
    for &byte in body {
        if !(32..=126).contains(&byte) || byte == b'*' || byte == b'$' {
            return Err("Invalid Fox frame body".into());
        }
        xor ^= byte;
    }
    Ok(xor)
}

pub fn encode(body: &str) -> Result<Vec<u8>, String> {
    if body.is_empty() || body.len() > MAX_FRAME - 6 {
        return Err("Invalid Fox frame length".into());
    }
    let xor = checksum(body.as_bytes())?;
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut frame = Vec::with_capacity(body.len() + 6);
    frame.push(b'$');
    frame.extend_from_slice(body.as_bytes());
    frame.extend_from_slice(&[b'*', HEX[(xor >> 4) as usize], HEX[(xor & 15) as usize], b'\r', b'\n']);
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragmented_coalesced_and_bad_frames_are_validated() {
        let mut framer = Framer::default();
        let mut frames = Vec::new();
        let frame = b"$FASTATUS,0*0F\r\n";
        for b in &frame[..frame.len() - 1] {
            framer.push(&[*b], |body| frames.push(body)).unwrap();
            assert!(frames.is_empty());
        }
        framer.push(b"\n", |body| frames.push(body)).unwrap();
        assert_eq!(frames, ["FASTATUS,0"]);
        frames.clear();
        framer.push(&[frame.as_slice(), frame.as_slice()].concat(), |body| frames.push(body)).unwrap();
        assert_eq!(frames, ["FASTATUS,0", "FASTATUS,0"]);
        assert!(framer.push(b"$FASTATUS,0*00\r\n", |_| panic!("invalid frame must not be delivered")).is_err());
        frames.clear();
        framer.push(frame, |body| frames.push(body)).unwrap();
        assert_eq!(frames, ["FASTATUS,0"]);
        assert_eq!(encode("FASTATUS,0").unwrap(), frame);
    }

    #[test]
    fn malformed_boundaries_ascii_and_size_are_rejected() {
        for body in ["bad*body", "bad$body", "bad\nbody", "非ASCII"] {
            assert!(encode(body).is_err());
        }
        for frame in [b"FASTATUS,0*0F\r\n".as_slice(), b"$FASTATUS,0*0G\r\n", b"$FASTATUS,0*0F\n", b"$FASTATUS,0*0F\r\xFF\n"] {
            assert!(Framer::default().push(frame, |_| panic!("invalid frame must not be delivered")).is_err());
        }
        let mut framer = Framer::default();
        assert!(framer.push(&vec![b'A'; MAX_FRAME + 1], |_| panic!("oversized frame must not be delivered")).is_err());
        let mut frames = Vec::new();
        framer.push(&encode("FASTATUS,0").unwrap(), |body| frames.push(body)).unwrap();
        assert_eq!(frames, ["FASTATUS,0"]);
    }

    #[test]
    fn valid_prefix_is_delivered_before_first_error_and_later_bytes_are_ignored() {
        let valid = encode("FAMOVE,0,B,1^3^15^B").unwrap();
        for invalid in [
            b"$BAD*00\r\n".to_vec(),
            b"BAD*47\r\n".to_vec(),
            b"$BA$D*00\r\n".to_vec(),
            b"$BAD*00\r\xFF\n".to_vec(),
            vec![b'A'; MAX_FRAME + 1],
        ] {
            let mut framer = Framer::default();
            let mut frames = Vec::new();
            let bytes = [valid.as_slice(), invalid.as_slice(), valid.as_slice()].concat();
            assert!(framer.push(&bytes, |body| frames.push(body)).is_err());
            assert_eq!(frames, ["FAMOVE,0,B,1^3^15^B"]);
            frames.clear();
            framer.push(&valid, |body| frames.push(body)).unwrap();
            assert_eq!(frames, ["FAMOVE,0,B,1^3^15^B"], "error must clear partial input");
        }
    }

    #[test]
    fn valid_prefix_survives_checksum_error_at_every_fragment_boundary() {
        let valid = encode("FAMOVE,0,B,1^3^15^B").unwrap();
        let bytes = [valid.as_slice(), b"$BAD*00\r\n", valid.as_slice()].concat();
        for split in 0..=bytes.len() {
            let mut framer = Framer::default();
            let mut frames = Vec::new();
            let result = framer.push(&bytes[..split], |body| frames.push(body))
                .and_then(|()| framer.push(&bytes[split..], |body| frames.push(body)));
            assert_eq!(result, Err("Invalid Fox checksum".into()), "split at {split}");
            assert_eq!(frames, ["FAMOVE,0,B,1^3^15^B"], "split at {split}");
        }
    }
}
