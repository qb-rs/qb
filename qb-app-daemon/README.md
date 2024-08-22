## qb-app-daemon

This binary starts a daemon that manages a master
and listens on a socket for a controlling task, which
its instructions it then further processes, allowing
the controlling task to manage the daemon over IPC.

This binary is a daemon, meaning it should run somewhere
in the background and only should be interacted by the user
over other tools, which use the socket to control the daemon.

### Running

- Using `cargo run` (for development)
```sh
$ cargo run --bin qb-daemon -- <args>
```
- Or [install qb-daemon locally](#installation)

### Installation

- Using `cargo install`
```sh
# In the project root directory
$ cargo install --path qb-daemon
# Make sure that ~/.cargo/bin is in $PATH
$ qb-daemon <args>
```
- Build manually
```sh
# In the project root directory
$ cargo build --release --bin qb-daemon
$ target/release/qb-daemon <args>
```

### Commands

See: `qb-daemon --help`
```
$ qb-daemon --help
Usage: qb-daemon [OPTIONS]

Options:
      --ípc          Bind to a socket for IPC [default]
      --no-ipc       Do not bind to a socket for IPC
      --stdio        Use STDIN/STDOUT for controlling (disables std logging)
      --no-stdio     Do not use STDIN/STDOUT for controlling [default]
  -p, --path <PATH>  The path, where the daemon stores its files [default: ./run/daemon1]
  -h, --help         Print help
  -V, --version      Print version
```

You can use the LOG_LEVEL environment variable to specify which log level to use:

command prefix|level description
---|---
LOG_LEVEL=trace|Designates very low priority, often extremely verbose, information.
LOG_LEVEL=debug|Designates lower priority information.
LOG_LEVEL=info|Designates useful information.
LOG_LEVEL=warn|Designates hazardous situations.
LOG_LEVEL=error|Designates very serious errors.

----

&copy; 2024 The QuixByte Project Authors - All Rights Reserved
