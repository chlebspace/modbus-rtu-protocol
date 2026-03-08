# modbus-rtu-protocol

This crate provides helpers for building and decoding standard Modbus RTU request and response packets.
It is a fork of the [modbus-rtu crate by Jababa](https://github.com/im-jababa/rust-modbus-rtu).

## Usage

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
