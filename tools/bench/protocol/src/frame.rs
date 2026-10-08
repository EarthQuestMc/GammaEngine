//! Framing: `VarInt length | VarInt packet id | body`, the length covering id and body.
//!
//! Protocol 5 has no compression, and encryption only exists in online mode, so a frame on the
//! wire is exactly this.

use crate::packets::Packet;
use crate::varint;
use crate::{Error, Result};

/// Largest frame length the 1.7.10 splitter accepts: its length prefix is at most 3 bytes.
pub const MAX_FRAME_LEN: usize = 0x1f_ffff;

/// Appends one frame holding `id` and `body` to `out`.
pub fn write_frame(out: &mut Vec<u8>, id: i32, body: &[u8]) -> Result<()> {
    let len = varint::varint_len(id) + body.len();
    if len > MAX_FRAME_LEN {
        return Err(Error::FrameTooLarge {
            len,
            max: MAX_FRAME_LEN,
        });
    }
    out.reserve(varint::varint_len(len as i32) + len);
    varint::write_varint(out, len as i32);
    varint::write_varint(out, id);
    out.extend_from_slice(body);
    Ok(())
}

/// Appends the frame of a typed packet to `out`.
pub fn write_packet<P: Packet>(out: &mut Vec<u8>, packet: &P) -> Result<()> {
    write_frame(out, P::ID, &packet.encode_body()?)
}

/// The frame of a typed packet, ready to send.
pub fn encode_packet<P: Packet>(packet: &P) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    write_packet(&mut out, packet)?;
    Ok(out)
}

/// A complete frame, borrowed from the decoder until its next call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame<'a> {
    pub id: i32,
    /// The packet fields, after the id.
    pub body: &'a [u8],
    /// The whole frame as received, length prefix included, for relaying it unchanged.
    pub raw: &'a [u8],
}

/// What [`FrameDecoder::next_filtered`] produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoded<'a> {
    Frame(Frame<'a>),
    /// A frame the caller did not want. `len` covers id and body. Its bytes may still be on
    /// their way: the decoder drops them as they are fed.
    Skipped {
        id: i32,
        len: usize,
    },
}

/// Cuts a byte stream into frames.
///
/// Feed it whatever the socket returned, then pull frames until it returns `None`. Frames the
/// caller filters out are discarded as soon as their id is known, so large chunk packets are
/// never held in memory.
#[derive(Debug)]
pub struct FrameDecoder {
    buf: Vec<u8>,
    start: usize,
    skip_remaining: usize,
    max_len: usize,
}

impl Default for FrameDecoder {
    fn default() -> Self {
        FrameDecoder::new()
    }
}

impl FrameDecoder {
    pub fn new() -> Self {
        FrameDecoder::with_max_len(MAX_FRAME_LEN)
    }

    pub fn with_max_len(max_len: usize) -> Self {
        FrameDecoder {
            buf: Vec::new(),
            start: 0,
            skip_remaining: 0,
            max_len,
        }
    }

    /// Bytes received but not returned yet, excluding those of a frame being skipped.
    pub fn buffered(&self) -> usize {
        self.buf.len() - self.start
    }

    /// Bytes of a skipped frame still expected from the stream.
    pub fn skipping(&self) -> usize {
        self.skip_remaining
    }

    pub fn feed(&mut self, mut data: &[u8]) {
        if self.skip_remaining > 0 {
            let dropped = self.skip_remaining.min(data.len());
            self.skip_remaining -= dropped;
            data = &data[dropped..];
        }
        if data.is_empty() {
            return;
        }
        if self.start > 0 {
            self.buf.drain(..self.start);
            self.start = 0;
        }
        self.buf.extend_from_slice(data);
    }

    /// The next complete frame, or `None` until more bytes are fed.
    pub fn next_frame(&mut self) -> Result<Option<Frame<'_>>> {
        Ok(self.next_filtered(|_| true)?.map(|decoded| match decoded {
            Decoded::Frame(frame) => frame,
            Decoded::Skipped { .. } => unreachable!("nothing is filtered out"),
        }))
    }

    /// The next frame, or `None` until more bytes are fed. `keep` sees the packet id as soon as
    /// it is readable; frames it rejects come back as [`Decoded::Skipped`].
    pub fn next_filtered(&mut self, keep: impl FnOnce(i32) -> bool) -> Result<Option<Decoded<'_>>> {
        if self.skip_remaining > 0 {
            return Ok(None);
        }
        let avail = &self.buf[self.start..];
        let Some((len, len_size)) = varint::decode_varint(avail)? else {
            return Ok(None);
        };
        if len < 0 {
            return Err(Error::NegativeLength(i64::from(len)));
        }
        let len = len as usize;
        if len > self.max_len {
            return Err(Error::FrameTooLarge {
                len,
                max: self.max_len,
            });
        }
        if len == 0 {
            return Err(Error::EmptyFrame);
        }
        let after_len = &avail[len_size..];
        let in_frame = &after_len[..after_len.len().min(len)];
        let (id, id_size) = match varint::decode_varint(in_frame)? {
            Some(found) => found,
            None if in_frame.len() == len => return Err(Error::EmptyFrame),
            None => return Ok(None),
        };
        let total = len_size + len;
        let wanted = keep(id);
        if avail.len() >= total {
            let begin = self.start;
            self.start += total;
            if !wanted {
                return Ok(Some(Decoded::Skipped { id, len }));
            }
            return Ok(Some(Decoded::Frame(Frame {
                id,
                body: &self.buf[begin + len_size + id_size..begin + total],
                raw: &self.buf[begin..begin + total],
            })));
        }
        if !wanted {
            self.skip_remaining = total - avail.len();
            self.buf.clear();
            self.start = 0;
            return Ok(Some(Decoded::Skipped { id, len }));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(id: i32, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        write_frame(&mut out, id, body).unwrap();
        out
    }

    #[test]
    fn write_frame_layout() {
        assert_eq!(frame(0x00, &[]), vec![0x01, 0x00]);
        assert_eq!(frame(0x3f, &[1, 2, 3]), vec![0x04, 0x3f, 1, 2, 3]);
        let big = vec![7u8; 300];
        let encoded = frame(0x26, &big);
        assert_eq!(&encoded[..3], &[0xad, 0x02, 0x26]);
        assert_eq!(encoded.len(), 2 + 301);
    }

    #[test]
    fn round_trip_byte_by_byte() {
        let frames: Vec<(i32, Vec<u8>)> = vec![
            (0x00, vec![]),
            (0x08, (0..41).collect()),
            (0x40, vec![9; 200]),
            (0x3f, vec![1]),
        ];
        let mut stream = Vec::new();
        for (id, body) in &frames {
            write_frame(&mut stream, *id, body).unwrap();
        }
        let mut decoder = FrameDecoder::new();
        let mut got = Vec::new();
        for byte in &stream {
            decoder.feed(std::slice::from_ref(byte));
            while let Some(frame) = decoder.next_frame().unwrap() {
                got.push((frame.id, frame.body.to_vec()));
            }
        }
        assert_eq!(got, frames);
        assert_eq!(decoder.buffered(), 0);
    }

    #[test]
    fn raw_is_the_whole_frame() {
        let bytes = frame(0x21, &[5, 6]);
        let mut decoder = FrameDecoder::new();
        decoder.feed(&bytes);
        let frame = decoder.next_frame().unwrap().unwrap();
        assert_eq!(frame.raw, &bytes[..]);
        assert_eq!(frame.body, &[5, 6]);
    }

    #[test]
    fn skipped_frames_are_not_buffered() {
        let mut stream = frame(0x26, &vec![0xaa; 10_000]);
        stream.extend(frame(0x00, &[0, 0, 0, 42]));
        let mut decoder = FrameDecoder::new();
        let mut events = Vec::new();
        for chunk in stream.chunks(1000) {
            decoder.feed(chunk);
            assert!(decoder.buffered() <= 1000);
            while let Some(decoded) = decoder.next_filtered(|id| id != 0x26).unwrap() {
                events.push(match decoded {
                    Decoded::Frame(f) => format!("frame {:#04x} {:?}", f.id, f.body),
                    Decoded::Skipped { id, len } => format!("skipped {id:#04x} {len}"),
                });
            }
        }
        assert_eq!(
            events,
            vec!["skipped 0x26 10001", "frame 0x00 [0, 0, 0, 42]"]
        );
        assert_eq!(decoder.skipping(), 0);
    }

    #[test]
    fn complete_skipped_frame_in_buffer() {
        let mut stream = frame(0x21, &[1, 2, 3]);
        stream.extend(frame(0x01, &[4]));
        let mut decoder = FrameDecoder::new();
        decoder.feed(&stream);
        assert_eq!(
            decoder.next_filtered(|id| id != 0x21).unwrap(),
            Some(Decoded::Skipped { id: 0x21, len: 4 })
        );
        let next = decoder.next_filtered(|id| id != 0x21).unwrap();
        assert!(matches!(
            next,
            Some(Decoded::Frame(Frame {
                id: 0x01,
                body: &[4],
                ..
            }))
        ));
        assert_eq!(decoder.next_frame().unwrap(), None);
    }

    #[test]
    fn malformed_frames() {
        let mut decoder = FrameDecoder::new();
        decoder.feed(&[0x00]);
        assert_eq!(decoder.next_frame(), Err(Error::EmptyFrame));

        let mut decoder = FrameDecoder::with_max_len(100);
        decoder.feed(&[0xe5, 0x00]);
        assert_eq!(
            decoder.next_frame(),
            Err(Error::FrameTooLarge { len: 101, max: 100 })
        );

        let mut decoder = FrameDecoder::new();
        decoder.feed(&[0xff, 0xff, 0xff, 0xff, 0x0f]);
        assert_eq!(decoder.next_frame(), Err(Error::NegativeLength(-1)));

        // A one-byte frame whose id announces a continuation byte it does not have.
        let mut decoder = FrameDecoder::new();
        decoder.feed(&[0x01, 0x80, 0x00]);
        assert_eq!(decoder.next_frame(), Err(Error::EmptyFrame));

        let mut decoder = FrameDecoder::new();
        decoder.feed(&[0x05, 0x01]);
        assert_eq!(decoder.next_frame(), Ok(None));
    }

    #[test]
    fn oversized_frame_is_refused_on_write() {
        let body = vec![0u8; MAX_FRAME_LEN];
        assert!(matches!(
            write_frame(&mut Vec::new(), 0x26, &body),
            Err(Error::FrameTooLarge { .. })
        ));
    }
}
