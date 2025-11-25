//! Async Modbus RTU master that queues requests and drives a single worker task.
//! Useful when multiple async callers need serialized access to one serial link.
use std::sync::Arc;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, sync::{mpsc, oneshot}, task::JoinHandle, time::Instant};
use tokio_serial::SerialPortBuilderExt;
use crate::{Function, Request, Response};
use serialport::SerialPort;


/// Multi-producer async master backed by a single worker task.
pub struct QueuedMaster {
    handle: JoinHandle<()>,
    sender: mpsc::Sender<Job>,
}

/// Packet of work sent into the worker loop.
struct Job {
    request: OwnedRequest,
    baud_rate: u32,
    respond_to: oneshot::Sender<Result<Response, crate::error::Error>>,
}

/// Owned copy of a request so the worker outlives the caller borrow.
struct OwnedRequest {
    modbus_id: u8,
    function: Function,
    timeout: core::time::Duration,
}

impl OwnedRequest {
    /// Clone the borrowed request into an owned form.
    fn from_borrowed(req: &Request<'_>) -> Self {
        Self {
            modbus_id: req.modbus_id(),
            function: req.function().clone(),
            timeout: req.timeout(),
        }
    }

    /// Rebuild a borrowed request view from the owned pieces.
    fn as_request(&self) -> Request<'_> {
        Request::new(self.modbus_id, &self.function, self.timeout)
    }
}

impl QueuedMaster {
    /// Build a queued master that spawns a worker task for one serial port.
    ///
    /// `buffer` is the queue depth of the internal MPSC channel; requests are
    /// buffered up to this limit. Passing 0 will panic in
    /// `tokio::sync::mpsc::channel`.
    /// 
    pub async fn new_rs485(path: &str, baud_rate: u32, buffer: usize) -> tokio_serial::Result<Arc<Self>> {
        let port = tokio_serial::new(path, baud_rate)
            .data_bits(tokio_serial::DataBits::Eight)
            .parity(tokio_serial::Parity::None)
            .stop_bits(tokio_serial::StopBits::One)
            .timeout(Self::idle_time_rs485(baud_rate))
            .open_native_async()?;
        let (sender, receiver) = mpsc::channel::<Job>(buffer);
        let handle = tokio::task::spawn(Self::task(port, baud_rate, receiver));
        Ok(Arc::new(Self {
            handle,
            sender,
        }))
    }

    /// Enqueue a request and wait for its response (or a broadcast ack).
    pub async fn send(&self, req: &Request<'_>, baud_rate: u32) -> Result<Response, crate::error::Error> {
        let (sender, receiver) = oneshot::channel::<Result<Response, crate::error::Error>>();
        let job = Job {
            request: OwnedRequest::from_borrowed(req),
            baud_rate,
            respond_to: sender,
        };
        self.sender
            .send(job)
            .await
            .map_err(|_| crate::error::Error::IO(std::io::ErrorKind::BrokenPipe.into()))?;
        receiver
            .await
            .map_err(|_| crate::error::Error::IO(std::io::ErrorKind::BrokenPipe.into()))?
    }

    /// Worker loop that serializes access to the serial port.
    async fn task(mut port: tokio_serial::SerialStream, baud_rate: u32, mut receiver: mpsc::Receiver<Job>) {
        let mut baud_rate = baud_rate;
        let mut last_tx: Instant = Instant::now();
        while let Some(Job { request, baud_rate: req_baud_rate, respond_to }) = receiver.recv().await {
            let req = request.as_request();
            if req_baud_rate != baud_rate {
                if let Err(e) = port.set_baud_rate(req_baud_rate) {
                    let _ = respond_to.send(Err(crate::error::Error::IO(e.into())));
                    continue;
                }
                baud_rate = req_baud_rate;
            }

            let idle_until = last_tx + Self::idle_time_rs485(baud_rate);
            let now = tokio::time::Instant::now();
            if idle_until > now {
                tokio::time::sleep_until(idle_until).await;
            }

            let frame = match req.to_bytes() {
                Ok(frame) => frame,
                Err(e) => {
                    let _ = respond_to.send(Err(crate::error::Error::Request(e)));
                    continue;
                },
            };

            if let Err(e) = port.clear(serialport::ClearBuffer::Output) {
                let _ = respond_to.send(Err(crate::error::Error::IO(e.into())));
                continue;
            }
            if let Err(e) = Self::write(&mut port, &frame).await {
                let _ = respond_to.send(Err(e));
                continue;
            }
            last_tx = tokio::time::Instant::now();

            if req.is_broadcasting() {
                let _ = respond_to.send(Ok(Response::Success));
                continue;
            }

            let post_tx_idle = Self::idle_time_rs485(baud_rate);
            tokio::time::sleep(post_tx_idle).await;
            let mut buf: [u8; 256] = [0; 256];
            let len = match Self::read(&mut port, &mut buf, req.timeout(), req.function().expected_len()).await {
                Ok(len) => len,
                Err(e) => {
                    let _ = respond_to.send(Err(e));
                    continue;
                },
            };
            if len == 0 {
                let _ = respond_to.send(Err(crate::error::Error::IO(std::io::ErrorKind::TimedOut.into())));
                continue;
            }
            let res = Response::from_bytes(&req, &buf[0..len]).map_err(|e| crate::error::Error::Response(e));
            let _ = respond_to.send(res);
        }
    }

    /// Writes a Modbus frame to the port and flushes it.
    async fn write(port: &mut tokio_serial::SerialStream, frame: &[u8]) -> Result<(), crate::error::Error> {
        port
            .write_all(frame)
            .await
            .map_err(|e| crate::error::Error::IO(e))?;
        port
            .flush()
            .await
            .map_err(|e| crate::error::Error::IO(e))?;
        
        Ok(())
    }

    /// Reads bytes until the slave stops responding or `buf` fills up.
    async fn read(
        port: &mut tokio_serial::SerialStream,
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
                tokio::time::timeout(remaining, port.read(&mut buf[len..])).await;
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


impl Drop for QueuedMaster {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
