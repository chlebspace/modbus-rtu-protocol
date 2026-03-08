/// ## Function
///
/// Represents a Modbus RTU function request along with the data required to
/// encode it into a protocol-compliant frame.
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Function {
    /// Read Coils `(0x01)`
    ReadCoils {
        starting_address: u16,
        quantity: u16,
    },

    /// Read Discrete Inputs `(0x02)`
    ReadDiscreteInputs {
        starting_address: u16,
        quantity: u16,
    },

    /// Read Holding Registers `(0x03)`
    ReadHoldingRegisters {
        starting_address: u16,
        quantity: u16,
    },

    /// Read Input Registers `(0x04)`
    ReadInputRegisters {
        starting_address: u16,
        quantity: u16,
    },

    /// Write Single Coil `(0x05)`
    WriteSingleCoil { address: u16, value: bool },

    /// Write Single Register `(0x06)`
    WriteSingleRegister { address: u16, value: u16 },

    /// Write Multiple Coils `(0x0F)`
    WriteMultipleCoils {
        starting_address: u16,
        value: Box<[bool]>,
    },

    /// Write Multiple Registers `(0x10)`
    WriteMultipleRegisters {
        starting_address: u16,
        value: Box<[u16]>,
    },
}

impl Function {
    /// Returns the [`FunctionKind`] associated with this request.
    ///
    /// ---
    /// # Examples
    /// ```rust
    /// use modbus_rtu::{Function, FunctionKind};
    ///
    /// let function = Function::ReadCoils { starting_address: 0, quantity: 2 };
    /// assert_eq!(function.kind(), FunctionKind::ReadCoils);
    /// ```
    ///
    pub const fn kind(&self) -> crate::FunctionKind {
        use crate::FunctionKind;
        match self {
            Function::ReadCoils { .. } => FunctionKind::ReadCoils,
            Function::ReadDiscreteInputs { .. } => FunctionKind::ReadDiscreteInputs,
            Function::ReadHoldingRegisters { .. } => FunctionKind::ReadHoldingRegisters,
            Function::ReadInputRegisters { .. } => FunctionKind::ReadInputRegisters,
            Function::WriteSingleCoil { .. } => FunctionKind::WriteSingleCoil,
            Function::WriteSingleRegister { .. } => FunctionKind::WriteSingleRegister,
            Function::WriteMultipleCoils { .. } => FunctionKind::WriteMultipleCoils,
            Function::WriteMultipleRegisters { .. } => FunctionKind::WriteMultipleRegisters,
        }
    }

    /// Returns the Modbus RTU function code for this request.
    ///
    /// ---
    /// # Examples
    /// ```rust
    /// use modbus_rtu::Function;
    ///
    /// let function = Function::WriteSingleRegister { address: 0x10, value: 0x1234 };
    /// assert_eq!(function.as_code(), 0x06);
    /// ```
    ///
    pub const fn as_code(&self) -> u8 {
        self.kind().as_code()
    }

    /// Serializes this function into a Modbus RTU payload (function code + data).
    ///
    /// Returns [`FunctionError`](crate::error::FunctionError) when the generated
    /// payload would exceed the 256-byte packet limit imposed by the Modbus RTU
    /// specification.
    ///
    /// # Examples
    /// ```ignore
    /// use modbus_rtu::Function;
    ///
    /// let function = Function::WriteSingleCoil { address: 0x0025, value: true };
    /// let bytes = function.to_bytes().unwrap();
    /// assert_eq!(&bytes[..], &[0x05, 0x00, 0x25, 0xFF, 0x00]);
    /// ```
    ///
    pub(crate) fn to_bytes(&self) -> Result<Box<[u8]>, crate::error::RequestPacketError> {
        let mut buf: Vec<u8> = Vec::with_capacity(5);
        buf.push(self.kind().as_code());
        match self {
            Function::ReadCoils {
                starting_address,
                quantity,
            }
            | Function::ReadDiscreteInputs {
                starting_address,
                quantity,
            } => {
                #[cfg(not(feature = "unlimited_packet_size"))]
                {
                    if *quantity > 2008 {
                        return Err(crate::error::RequestPacketError::ResponseWillTooBig);
                    }
                }
                buf.extend_from_slice(&starting_address.to_be_bytes());
                buf.extend_from_slice(&quantity.to_be_bytes());
            }
            Function::ReadHoldingRegisters {
                starting_address,
                quantity,
            }
            | Function::ReadInputRegisters {
                starting_address,
                quantity,
            } => {
                #[cfg(not(feature = "unlimited_packet_size"))]
                {
                    if *quantity > 125 {
                        return Err(crate::error::RequestPacketError::ResponseWillTooBig);
                    }
                }
                buf.extend_from_slice(&starting_address.to_be_bytes());
                buf.extend_from_slice(&quantity.to_be_bytes());
            }
            Function::WriteSingleCoil { address, value } => {
                buf.extend_from_slice(&address.to_be_bytes());
                buf.push(if *value { 0xFF } else { 0x00 });
                buf.push(0x00);
            }
            Function::WriteSingleRegister { address, value } => {
                buf.extend_from_slice(&address.to_be_bytes());
                buf.extend_from_slice(&value.to_be_bytes());
            }
            Function::WriteMultipleCoils {
                starting_address,
                value,
            } => {
                let quantity = value.len() as u16;
                #[cfg(not(feature = "unlimited_packet_size"))]
                {
                    if quantity > 1976 {
                        return Err(crate::error::RequestPacketError::RequestTooBig);
                    }
                }
                let byte_count = quantity.div_ceil(8) as u8;
                buf.extend_from_slice(&starting_address.to_be_bytes());
                buf.extend_from_slice(&quantity.to_be_bytes());
                buf.push(byte_count);
                for chunk in value.chunks(8) {
                    let mut byte: u8 = 0x00;
                    for (i, value) in chunk.iter().enumerate() {
                        if *value {
                            byte |= 0b1 << i;
                        } else {
                            byte &= !(0b1 << i);
                        }
                    }
                    buf.push(byte);
                }
            }
            Function::WriteMultipleRegisters {
                starting_address,
                value,
            } => {
                let quantity = value.len() as u16;
                #[cfg(not(feature = "unlimited_packet_size"))]
                {
                    if quantity > 123 {
                        return Err(crate::error::RequestPacketError::RequestTooBig);
                    }
                }
                let byte_count = (quantity * 2) as u8;
                buf.extend_from_slice(&starting_address.to_be_bytes());
                buf.extend_from_slice(&quantity.to_be_bytes());
                buf.push(byte_count);
                for each in value {
                    buf.extend_from_slice(&each.to_be_bytes());
                }
            }
        }
        Ok(buf.into_boxed_slice())
    }

    /// Returns the minimum expected response length for this function.
    ///
    /// This helps callers pre-allocate receive buffers before the Modbus frame
    /// arrives.
    ///
    /// ---
    /// # Examples
    /// ```rust
    /// use modbus_rtu::Function;
    ///
    /// let func = Function::ReadHoldingRegisters { starting_address: 0, quantity: 2 };
    /// assert_eq!(func.expected_len(), 5 + (2 * 2));
    /// ```
    ///
    pub const fn expected_len(&self) -> usize {
        match self {
            Function::ReadCoils { quantity, .. }
            | Function::ReadDiscreteInputs { quantity, .. } => 5 + (*quantity as usize).div_ceil(8),
            Function::ReadHoldingRegisters { quantity, .. }
            | Function::ReadInputRegisters { quantity, .. } => 5 + (*quantity as usize * 2),
            Function::WriteSingleCoil { .. }
            | Function::WriteSingleRegister { .. }
            | Function::WriteMultipleCoils { .. }
            | Function::WriteMultipleRegisters { .. } => 8,
        }
    }
}
