//! modbus-rtu crate error types

mod request_packet;
pub use request_packet::*;

mod response_packet;
use crate::Exception;
pub use response_packet::*;

/// Error type that captures failures encountered when talking to a
/// Modbus RTU device.
#[derive(Debug)]
pub enum Error {
    /// The slave device replied with a Modbus exception response.
    Exception(Exception),

    /// The request packet could not be constructed; see [`RequestPacketError`]
    /// for details.
    Request(RequestPacketError),

    /// The response packet failed validation or decoding; see
    /// [`ResponsePacketError`] for the specific cause.
    Response(ResponsePacketError),

    /// Any I/O error surfaced by the underlying serial transport.
    IO(std::io::Error),
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Exception(exception) => write!(f, "device responsed {exception}"),
            Error::Request(request_packet_error) => write!(f, "{request_packet_error}"),
            Error::Response(response_packet_error) => write!(f, "{response_packet_error}"),
            Error::IO(error) => write!(f, "{error}"),
        }
    }
}

impl core::error::Error for Error {}
