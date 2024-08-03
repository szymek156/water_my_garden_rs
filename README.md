# source of truth
https://docs.esp-rs.org/book/writing-your-own-application/
https://docs.esp-rs.org/std-training/
https://docs.esp-rs.org/no_std-training/
community on matrix https://matrix.to/#/#esp-rs:matrix.org

# troubleshooting
## Cannot flash
If flashing hangs on connecting to a device - reset the board using RST button
## Toolchain decided to break
There is a message in a stacktrace:
```
Requirement 'setuptools<71.0.1,>=21' was not met. Installed version: 71.1.0
```
- Go to `cd /home/szym/esp/rust_on_esp/water-my-garden-rs/.embuild/espressif/python_env/idf5.2_py3.11_env/lib/python3.11/site-packages`
- rm all dirs with ~ as a prefix
- install setuptools `/home/szym/esp/rust_on_esp/water-my-garden-rs/.embuild/espressif/python_env/idf5.2_py3.11_env/bin/pip install setuptools==71.0.0`
- confirm installation `/home/szym/esp/rust_on_esp/water-my-garden-rs/.embuild/espressif/python_env/idf5.2_py3.11_env/bin/pip show setuptools`


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

