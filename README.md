# modbus-rtu

[![Github](https://img.shields.io/badge/github-im--jababa%2Frust--modbus--rtu-8da0cb?style=for-the-badge&labelColor=555555&logo=github)](https://github.com/im-jababa/rust-modbus-rtu)
[![Crates.io](https://img.shields.io/badge/crates.io-modbus--rtu-fc8d62?style=for-the-badge&labelColor=555555&logo=rust)](https://crates.io/crates/modbus-rtu)
[![Docs.rs](https://img.shields.io/badge/docs.rs-modbus--rtu-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs)](https://docs.rs/modbus-rtu)
[![Build](https://img.shields.io/github/actions/workflow/status/im-jababa/rust-modbus-rtu/rust.yml?branch=main&style=for-the-badge)](https://github.com/im-jababa/rust-modbus-rtu/actions?query=branch%3Amain)

This crate provides helpers for building and decoding standard Modbus RTU request and response packets.
It now ships with a synchronous `Master` that can talk to a serial port directly, while still
exposing the lower-level building blocks for applications that prefer to manage framing themselves.

---

# Usage

## High-level master (auto write/read)

```rust
use modbus_rtu::{Function, Master, Request};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut master = Master::new_rs485("/dev/ttyUSB0", 19_200)?;

    let func = Function::ReadHoldingRegisters { starting_address: 0x0000, quantity: 2 };
    let request = Request::new(0x01, &func, std::time::Duration::from_millis(200));

    let response = master.send(&request)?;
    println!("response: {response:?}");
    Ok(())
}
```

The master enforces the Modbus RTU silent interval (T3.5) before/after each transmission,
flushes the TX buffer, reads until the slave stops talking, and automatically decodes the reply.

---

## Async master (Tokio)

`AsyncMaster` ships enabled by default (feature `async`). You only need to wire up a Tokio runtime. Disable
`async` in `Cargo.toml` if you prefer the smaller blocking-only build.

```toml
[dependencies]
modbus-rtu = "1.2"
tokio = { version = "1.38", features = ["rt", "macros"] }
```

```rust
use modbus_rtu::{Function, AsyncMaster, Request};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut master = AsyncMaster::new_rs485("/dev/ttyUSB0", 19_200)?;

    let func = Function::ReadHoldingRegisters { starting_address: 0x0000, quantity: 2 };
    let request = Request::new(0x01, &func, std::time::Duration::from_millis(200));

    let response = master.send(&request).await?;
    println!("response: {response:?}");
    Ok(())
}
```

The async master mirrors the synchronous behavior but uses async sleeps and I/O to maintain the
Modbus RTU silent interval between frames.

---

## Queued async master (shared worker)

`QueuedMaster` wraps a single async worker task and an MPSC queue so multiple async callers can share
one serial link. The `buffer` argument is the channel depth; requests are buffered up to that limit
and passing `0` will panic inside `tokio::sync::mpsc::channel`.

```rust
use std::time::Duration;
use modbus_rtu::{Function, QueuedMaster, Request};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // buffer=4 means up to four queued requests before senders await.
    let master = QueuedMaster::new_rs485("/dev/ttyUSB0", 38_400, 4).await?;

    let func = Function::ReadInputRegisters { starting_address: 0x0001, quantity: 12 };
    let req = Request::new(1, &func, Duration::from_millis(100));

    let response = master.send(&req, 38_400).await?;
    println!("response: {response:?}");
    Ok(())
}
```

You can clone the returned `Arc<QueuedMaster>` and call `send` from many tasks; the worker enforces
Modbus idle timing between frames and switches baud rate per request if needed.

---

## Opting out of the master to shrink binaries

The synchronous and async masters (and their serial dependencies) are enabled by default. If you only
need the packet-building utilities, disable default features in your `Cargo.toml`:

```toml
[dependencies]
modbus-rtu = { version = "1.2", default-features = false }
```

Then opt into what you need:

- Blocking master only (drops Tokio/async deps):
  ```toml
  modbus-rtu = { version = "1.2", default-features = false, features = ["master"] }
  ```
- Both masters (default behavior):
  ```toml
  modbus-rtu = { version = "1.2", default-features = false, features = ["master", "async"] }
  ```


---

## Manual packet construction

First, construct the function you want to issue.
The following example reads four input registers starting at address `0x1234`.

```rust
use modbus_rtu::Function;

let starting_address: u16 = 0x1234;
let quantity: usize = 4;
let function = Function::ReadInputRegisters { starting_address, quantity };
```

Next, build the request with the target device information and timeout.

```rust
use modbus_rtu::{Function, Request};

...

let modbus_id: u8 = 1;
let timeout: std::time::Duration = std::time::Duration::from_millis(100);
let request = Request::new(1, &function, timeout);
```

Finally, convert the request into a Modbus RTU frame.

```rust
...

let packet: Box<[u8]> = request.to_bytes().expect("Failed to build request packet");
```

You can now write `packet` through any transport of your choice (UART, TCP tunnel, etc.).

---

## Receiving

With the original request available, attempt to decode the response bytes as shown below.

```rust
use modbus_rtu::Response;

...

let bytes: &[u8] = ... ; // user-implemented receive logic

let response = Response::from_bytes(&request, bytes).expect("Failed to analyze response packet");

match response {
    Response::Value(value) => {
        let _ = value[0];   // value at address 0x1234
        let _ = value[1];   // value at address 0x1235
        let _ = value[2];   // value at address 0x1236
        let _ = value[3];   // value at address 0x1237
    },
    Response::Exception(e) => {
        eprintln!("device responded with exception: {e}");
    },
    _ => unreachable!(),
}
```
If you disable default features, re-enable both `master` and `async` to access `AsyncMaster`.
