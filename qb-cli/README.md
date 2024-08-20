## qb-cli

This CLI client connects to and manages the daemon through
the control messaging protocol defined in [qb-control](../qb-control/).

### Running

- Using `cargo run` (for development)
```sh
$ cargo run --bin qb-cli -- <args>
```
- Or [install qb-cli locally](#installation)

### Installation

- Using `cargo install`
```sh
# In the project root directory
$ cargo install --path qb-cli
# Make sure that ~/.cargo/bin is in $PATH
$ qb-cli <args>
```
- Build manually
```sh
# In the project root directory
$ cargo build --release --bin qb-cli
$ target/release/qb-cli <args>
```

### Commands

See: `qb-cli --help`
```
$ qb-cli --help
Usage: qb-cli <COMMAND>

Commands:
  list   List the connected extensions
  add    Add an extension
  rm     Remove an extension
  start  Start an extension
  stop   Stop an extension
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

----

&copy; 2024 The QuixByte Project Authors - All Rights Reserved
