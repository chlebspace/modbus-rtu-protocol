/// Represents the outcome of a Modbus RTU request, covering data reads, write
/// acknowledgements, and protocol exceptions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    /// A collection of coil/discrete input states returned by the device.
    Status(Box<[bool]>),

    /// A collection of register values returned by the device.
    Value(Box<[u16]>),

    /// Confirmation that a write request completed successfully.
    Success,

    /// A Modbus application exception reported by the device.
    Exception(crate::Exception),
}

impl Response {
    /// Decodes a Modbus RTU response frame into a [`Response`] value.
    ///
    /// ---
    /// # Arguments
    /// - `request`: The originating request used to validate address, function,
    ///   and quantity semantics.
    /// - `bytes`: Raw response frame including slave id, function code, payload,
    ///   and CRC.
    ///
    /// ---
    /// # Errors
    /// Returns [`ResponsePacketError`](crate::error::ResponsePacketError) when
    /// the frame does not pass validation (CRC mismatch, unexpected responder,
    /// malformed payload, etc.).
    ///
    /// ---
    /// # Examples
    /// ```rust
    /// use modbus_rtu::{Function, Request, Response};
    ///
    /// let function = Function::ReadInputRegisters { starting_address: 0x0000, quantity: 2 };
    /// let request = Request::new(0x01, &function, std::time::Duration::from_millis(100));
    /// let frame = [0x01, 0x04, 0x04, 0x00, 0x10, 0x00, 0x20, 0xFB, 0x99];
    ///
    /// let response = Response::from_bytes(&request, &frame).unwrap();
    /// match response {
    ///     Response::Value(values) => assert_eq!(&values[..], &[0x0010, 0x0020]),
    ///     _ => panic!("unexpected response variant"),
    /// }
    /// ```
    ///
    pub fn from_bytes(
        request: &crate::Request,
        bytes: &[u8],
    ) -> Result<Self, crate::error::ResponsePacketError> {
        // minimum length check
        let len = bytes.len();
        if len < 5 {
            return Err(crate::error::ResponsePacketError::TooShort(len));
        }

        // crc check
        crate::crc::validate(bytes)?;

        // exception check
        let function_code = bytes[1];
        if function_code & 0x80 != 0 {
            let code = bytes[2];
            return Ok(Self::Exception(crate::Exception::from_code(code)));
        }

        // modbus id check
        if bytes[0] != request.modbus_id() {
            return Err(crate::error::ResponsePacketError::UnexpectedResponder(
                bytes[0],
            ));
        }

        // function code check
        let function_kind = match crate::FunctionKind::from_code(function_code) {
            Some(kind) => kind,
            None => return Err(crate::error::ResponsePacketError::InvalidFormat),
        };
        if function_kind != request.function().kind() {
            return Err(crate::error::ResponsePacketError::InvalidFormat);
        }

        // trim
        let packet = &bytes[2..(len - 2)];

        // analyze
        match function_kind {
            crate::FunctionKind::ReadCoils | crate::FunctionKind::ReadDiscreteInputs => {
                let byte_count = packet[0];
                let quantity = match request.function() {
                    crate::Function::ReadCoils { quantity, .. }
                    | crate::Function::ReadDiscreteInputs { quantity, .. } => *quantity,
                    _ => unreachable!(),
                };
                if byte_count < (quantity as u8).div_ceil(8) {
                    return Err(crate::error::ResponsePacketError::InvalidFormat);
                }
                if packet.len() < byte_count as usize + 1 {
                    return Err(crate::error::ResponsePacketError::InvalidFormat);
                }
                let mut list: Vec<bool> = Vec::with_capacity(quantity as usize);
                for (i, byte) in packet[1..].iter().enumerate() {
                    for j in 0..8_usize {
                        if (i * 8) + j >= quantity as usize {
                            break;
                        }
                        let value = byte & (0b1 << j) != 0;
                        list.push(value);
                    }
                }
                Ok(Self::Status(list.into_boxed_slice()))
            }
            crate::FunctionKind::ReadHoldingRegisters | crate::FunctionKind::ReadInputRegisters => {
                let byte_count = packet[0];
                let quantity = match request.function() {
                    crate::Function::ReadHoldingRegisters { quantity, .. }
                    | crate::Function::ReadInputRegisters { quantity, .. } => *quantity,
                    _ => unreachable!(),
                };
                if byte_count < quantity as u8 * 2 {
                    return Err(crate::error::ResponsePacketError::InvalidFormat);
                }
                if packet.len() < byte_count as usize + 1 {
                    return Err(crate::error::ResponsePacketError::InvalidFormat);
                }
                let mut list: Vec<u16> = Vec::with_capacity(quantity as usize * 2);
                for i in 0..(quantity as usize) {
                    let hi = packet[1 + (i * 2)];
                    let lo = packet[2 + (i * 2)];
                    let value = u16::from_be_bytes([hi, lo]);
                    list.push(value);
                }
                Ok(Self::Value(list.into_boxed_slice()))
            }
            crate::FunctionKind::WriteSingleCoil | crate::FunctionKind::WriteSingleRegister => {
                if packet.len() != 4 {
                    return Err(crate::error::ResponsePacketError::InvalidFormat);
                }
                let (req_address, req_value) = match request.function() {
                    crate::Function::WriteSingleCoil { address, value } => {
                        (*address, if *value { 0xFF00 } else { 0x0000 })
                    }
                    crate::Function::WriteSingleRegister { address, value } => (*address, *value),
                    _ => unreachable!(),
                };
                let res_address = u16::from_be_bytes([packet[0], packet[1]]);
                let res_value = u16::from_be_bytes([packet[2], packet[3]]);
                if req_address != res_address || req_value != res_value {
                    return Err(crate::error::ResponsePacketError::InvalidFormat);
                }
                Ok(Self::Success)
            }
            crate::FunctionKind::WriteMultipleCoils
            | crate::FunctionKind::WriteMultipleRegisters => {
                if packet.len() != 4 {
                    return Err(crate::error::ResponsePacketError::InvalidFormat);
                }
                let (req_address, req_quantity) = match request.function() {
                    crate::Function::WriteMultipleCoils {
                        starting_address,
                        value,
                    } => (*starting_address, value.len() as u16),
                    crate::Function::WriteMultipleRegisters {
                        starting_address,
                        value,
                    } => (*starting_address, value.len() as u16),
                    _ => unreachable!(),
                };
                let res_address = u16::from_be_bytes([packet[0], packet[1]]);
                let res_quantity = u16::from_be_bytes([packet[2], packet[3]]);
                if req_address != res_address || req_quantity != res_quantity {
                    return Err(crate::error::ResponsePacketError::InvalidFormat);
                }
                Ok(Self::Success)
            }
        }
    }

    /// Returns `true` when the response indicates that the request succeeded.
    ///
    /// The method treats the Modbus `Acknowledge (0x05)` exception as success
    /// because it signals that the device accepted the request and will complete
    /// it asynchronously.
    ///
    /// ---
    /// # Examples
    /// ```rust
    /// use modbus_rtu::{Exception, Response};
    ///
    /// assert!(Response::Success.is_success());
    /// assert!(Response::Exception(Exception::Acknowledge).is_success());
    /// assert!(!Response::Exception(Exception::IllegalFunction).is_success());
    /// ```
    ///
    pub fn is_success(&self) -> bool {
        match self {
            Response::Status(_) | Response::Value(_) | Response::Success => true,
            Response::Exception(exception) => *exception == crate::Exception::Acknowledge,
        }
    }
}

impl core::fmt::Display for Response {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Response::Status(items) => format!("{:?}", items),
                Response::Value(items) => format!("{:?}", items),
                Response::Success => "Success".to_string(),
                Response::Exception(exception) => exception.to_string(),
            }
        )
    }
}
