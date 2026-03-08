/// Errors that can occur while validating and decoding a Modbus RTU response packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponsePacketError {
    /// The response frame is shorter than the minimum Modbus RTU length.
    TooShort(usize),

    /// Calculated CRC does not match the CRC bytes present in the frame.
    CRCMismatch { expected: u16, received: u16 },

    /// The response came from a different Modbus slave than the request targeted.
    UnexpectedResponder(u8),

    /// The payload failed structural validation (unexpected function code,
    /// byte count mismatch, etc.).
    InvalidFormat,
}

impl core::fmt::Display for ResponsePacketError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::TooShort(len) => format!(
                    "response packet too short; expected at least 5 bytes but received {len}."
                ),
                Self::CRCMismatch { expected, received } => format!(
                    "response CRC mismatch: expected 0x{expected:04X}, received 0x{received:04X}."
                ),
                Self::UnexpectedResponder(id) =>
                    format!("response came from unexpected Modbus slave id 0x{id:02X}."),
                Self::InvalidFormat => "response payload format is invalid.".to_string(),
            }
        )
    }
}

impl core::error::Error for ResponsePacketError {}
