use std::fmt;

/// Everything that can go wrong while encoding or decoding protocol data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The input ended before the value was complete.
    UnexpectedEof,
    /// A VarInt or VarLong used more bytes than its type (or the caller) allows.
    VarIntTooLong,
    /// A length prefix was negative.
    NegativeLength(i64),
    /// A string exceeds the limit the receiving side enforces.
    StringTooLong { max: usize, len: usize },
    /// String bytes are not valid UTF-8.
    InvalidUtf8,
    /// A frame announces more bytes than the protocol allows.
    FrameTooLarge { len: usize, max: usize },
    /// A frame has no room for its packet id.
    EmptyFrame,
    /// A payload does not fit the length field of its packet.
    PayloadTooLarge { len: usize, max: usize },
    /// A packet body has bytes left after its last field.
    TrailingBytes(usize),
    /// An `FML|HS` message starts with an unknown discriminator.
    UnknownDiscriminator(u8),
    /// A handshake message arrived in a state that cannot accept it.
    UnexpectedMessage(&'static str),
    /// A JSON document is malformed.
    Json { offset: usize, reason: &'static str },
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnexpectedEof => write!(f, "unexpected end of data"),
            Error::VarIntTooLong => write!(f, "VarInt too long"),
            Error::NegativeLength(len) => write!(f, "negative length {len}"),
            Error::StringTooLong { max, len } => {
                write!(f, "string of {len} exceeds the limit of {max}")
            }
            Error::InvalidUtf8 => write!(f, "string is not valid UTF-8"),
            Error::FrameTooLarge { len, max } => {
                write!(f, "frame of {len} bytes exceeds the limit of {max}")
            }
            Error::EmptyFrame => write!(f, "frame has no packet id"),
            Error::PayloadTooLarge { len, max } => {
                write!(f, "payload of {len} bytes exceeds the limit of {max}")
            }
            Error::TrailingBytes(n) => write!(f, "{n} unread bytes after the last field"),
            Error::UnknownDiscriminator(d) => write!(f, "unknown FML|HS discriminator {d:#04x}"),
            Error::UnexpectedMessage(what) => write!(f, "unexpected handshake message: {what}"),
            Error::Json { offset, reason } => write!(f, "invalid JSON at byte {offset}: {reason}"),
        }
    }
}

impl std::error::Error for Error {}
