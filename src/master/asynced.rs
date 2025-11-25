//! Async Modbus RTU master backed by the `tokio-serial` crate.
use crate::{Request, Response};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::SerialPortBuilderExt;
use serialport::SerialPort;


/// Async Modbus RTU master that honors Modbus idle timing rules between frames.
#[derive(Debug)]
pub struct AsyncMaster {
    /// Serial port handle used for request/response traffic.
    port: tokio_serial::SerialStream,

    /// Timestamp of the last transmitted frame, used to honor the 3.5-char gap.
    last_tx: tokio::time::Instant,

    /// Cached baud rate so higher-level code can inspect the active speed.
    baud_rate: u32,
}


impl AsyncMaster {
    /// Builds a master configured for an RS-485 style setup (8N1, async I/O).
    ///
    /// The port timeout is pinned to the Modbus RTU silent interval (T3.5) for
    /// the supplied baud rate so that the reader can detect frame boundaries.
    pub fn new_rs485(path: &str, baud_rate: u32) -> serialport::Result<Self> {
        let port = tokio_serial::new(path, baud_rate)
            .data_bits(tokio_serial::DataBits::Eight)
            .parity(tokio_serial::Parity::None)
            .stop_bits(tokio_serial::StopBits::One)
            .timeout(Self::idle_time_rs485(baud_rate))
            .open_native_async()?;
        Ok(Self {
            port,
            last_tx: tokio::time::Instant::now() - Self::idle_time_rs485(baud_rate),
            baud_rate,
        })
    }

    /// Returns the baud rate currently configured on the serial link.
    pub fn baud_rate(&self) -> u32 {
        self.baud_rate
    }

    /// Updates the serial baud rate and matching Modbus idle timeout.
    pub fn set_baudrate(&mut self, baud_rate: u32) -> serialport::Result<()> {
        self.port.set_baud_rate(baud_rate)?;
        self.port.set_timeout(Self::idle_time_rs485(baud_rate))?;
        self.baud_rate = baud_rate;
        self.last_tx = tokio::time::Instant::now();
        Ok(())
    }

    /// Sends a Modbus RTU request and waits for the corresponding response.
    ///
    /// Broadcast requests return immediately after the frame is flushed because
    /// the Modbus RTU spec forbids responses to slave id 0.
    pub async fn send(&mut self, req: &Request<'_>) -> Result<Response, crate::error::Error> {
        self.wait_for_idle_gap().await;
        let frame = req
            .to_bytes()
            .map_err(|e| crate::error::Error::Request(e))?;
        self.port
            .clear(serialport::ClearBuffer::Output)
            .map_err(|e| crate::error::Error::IO(e.into()))?;
        self.write(&frame).await?;
        if req.is_broadcasting() {
            return Ok(Response::Success);
        }
        let post_tx_idle = Self::idle_time_rs485(self.baud_rate);
        tokio::time::sleep(post_tx_idle).await;
        let mut buf: [u8; 256] = [0; 256];
        let len = self
            .read(&mut buf, req.timeout(), req.function().expected_len())
            .await?;
        if len == 0 {
            return Err(crate::error::Error::IO(
                std::io::ErrorKind::TimedOut.into(),
            ));
        }
        Response::from_bytes(req, &buf[0..len]).map_err(|e| crate::error::Error::Response(e))
    }

    /// Waits for the Modbus silent interval to elapse before transmitting.
    async fn wait_for_idle_gap(&self) {
        let idle_until = self.last_tx + Self::idle_time_rs485(self.baud_rate);
        let now = tokio::time::Instant::now();
        if idle_until > now {
            tokio::time::sleep_until(idle_until).await;
        }
    }

    /// Writes a Modbus frame to the serial port and records the transmit instant.
    async fn write(&mut self, frame: &[u8]) -> Result<(), crate::error::Error> {
        self.port
            .write_all(frame)
            .await
            .map_err(|e| crate::error::Error::IO(e))?;
        self.port
            .flush()
            .await
            .map_err(|e| crate::error::Error::IO(e))?;
        self.last_tx = tokio::time::Instant::now();
        Ok(())
    }

    /// Reads bytes until the slave stops responding or `buf` fills up.
    async fn read(
        &mut self,
        buf: &mut [u8],
        timeout: core::time::Duration,
        expected_len: usize,
    ) -> Result<usize, crate::error::Error> {
        let mut len: usize = 0;
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline.saturating_duration_since(now);
            let read_res =
                tokio::time::timeout(remaining, self.port.read(&mut buf[len..])).await;
            let n = match read_res {
                Ok(Ok(n)) => n,
                Ok(Err(ref e)) if e.kind() == std::io::ErrorKind::TimedOut => {
                    if len == 0 {
                        continue;
                    }
                    if len >= 5 && buf[1] & 0x80 != 0 {
                        break;
                    }
                    if len < expected_len {
                        continue;
                    }
                    break
                }
                Ok(Err(e)) => return Err(crate::error::Error::IO(e)),
                Err(_) => break,
            };
            len += n;
            if len >= buf.len() {
                break;
            }
        }
        Ok(len)
    }

    /// Computes the Modbus RTU T3.5 idle time for a link running 8N1 encoding.
    fn idle_time_rs485(baud_rate: u32) -> core::time::Duration {
        const BITS_PER_CHAR: f64 = 10.0;
        let seconds = 3.5 * BITS_PER_CHAR / baud_rate as f64;
        core::time::Duration::from_secs_f64(seconds)
    }
}
