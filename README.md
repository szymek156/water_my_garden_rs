# source of truth
https://docs.esp-rs.org/book/writing-your-own-application/

# troubleshooting
If flashing hangs on connecting to a device - reset the board using RST button

# Run
- `source export-esp.sh`
- `cargo run`

# Notes
`esp-idf-sys` - unsafe bindings to esp-idf SDK
`esp-idf-svc` - abstraction over `sys` crate
`esp-idf-hal` - implements traits from `rust-embedded/embedded-hal` (traits for I2C, SPI)

# create project with std
`cargo generate esp-rs/esp-idf-template cargo`

# create project without std `no_std`
`cargo generate esp-rs/esp-template`

