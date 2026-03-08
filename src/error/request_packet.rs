/// Errors that can occur while building a Modbus RTU request packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestPacketError {
    /// This error is raised when the function tries to produce a request packet
    /// that exceeds the Modbus RTU protocol's maximum packet length of 256 bytes.
    ///
    /// Requests that attempt to write too many values at once will exceed
    /// the 256-byte limit of the request packet.
    ///
    /// ---
    ///
    /// If you intentionally need to bypass the request packet length limit,
    /// enable the Cargo feature as shown below.
    ///
    /// ## Warning: packets produced with this feature enabled may fail during communication.
    ///
    /// ```ignore
    /// [dependencies]
    /// modbus-rtu = { version = "1.0", features = ["unlimited_packet_size"] }
    /// ```
    ///
    RequestTooBig,

    /// This error is raised when the expected response packet would exceed the
    /// Modbus RTU protocol's maximum packet length of 256 bytes.
    ///
    /// ---
    ///
    /// If you intentionally need to bypass the response packet length limit,
    /// enable the Cargo feature as shown below.
    ///
    /// ## Warning: packets produced with this feature enabled may fail during communication.
    ///
    /// ```ignore
    /// [dependencies]
    /// modbus-rtu = { version = "1.0", features = ["unlimited_packet_size"] }
    /// ```
    ///
    ResponseWillTooBig,

    /// This error occurs when attempting to broadcast a function that does not
    /// support broadcasting (e.g., 0x01, 0x03).
    CannotBroadcast,
}

impl core::fmt::Display for RequestPacketError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::RequestTooBig => "request packet exceeds 256-byte.",
            Self::ResponseWillTooBig => "expected response packet exceeds 256-byte.",
            Self::CannotBroadcast => "this function does not support Modbus RTU broadcasting.",
        };
        f.write_str(message)
    }
}

impl core::error::Error for RequestPacketError {}
