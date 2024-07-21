<img src="https://raw.githubusercontent.com/qb-rs/.github/main/profile/quixbyte_full_light.svg#gh-dark-mode-only" alt="QuixByte" width="400px"/>
<img src="https://raw.githubusercontent.com/qb-rs/.github/main/profile/quixbyte_full.svg#gh-light-mode-only" alt="QuixByte" width="400px"/>

```
-- About

QuixByte is an opensource file service, which allows
users to quickly synchronize and backup their files
across multiple devices.


-- Joining the Project

We are happy to accept contributions. If you are looking
to join this project as a maintainer make sure to contribute
something meaningful once or twice and then get in touch.
(lucasbirkert@gmail.com)
```

## Table of Contents

1. [Goals](#goals)
2. [Platform Support](#platform-support)
3. [QBI Support](#qbi-support)
4. [Quickstart](#quickstart)
5. [Terminology](#terminology)

## Goals

- Fast, resilient sync
- Memory safety, no crashes
- Low resource usage
- Support of many different [storage services](#terminology-storage-service) ([current state](#qbi-support))
- Extensibility, allow external processes to act as [QBI](#terminology-interface)s
- Wide platform support ([current state](#platform-support))
- Entirely opensource

## Platform Support

<!-- TODO: tidy this -->

platform|arch|support|planned
---|---|---|---
Linux|x86_64|full, tested|yes
Linux|any|untested, should work|yes
Windows|any|currently no support|yes
Mac-OS|any|untested, should work|yes
Android|any|unknown|yes
iOS|any|unknown|yes

## QBI Support

<!-- TODO: tidy this -->

service|description|support|planned
---|---|---|---
qbi-local|sync to local folder|yes|yes
qbi-rtc|sync via WebRTC|unimplemented|yes
qbi-server|start server|unimplemented|yes
qbi-client|sync to server|unimplemented|yes
qbi-gdrive|sync to Google Drive|unimplemented|yes
qbi-dropbox|sync to Dropbox|unimplemented|yes

## Quickstart

1. Install the latest version of rust: https://rustup.rs/
2. Clone the repository:
```sh
$ git clone --depth 1 https://github.com/qb-rs/qb
$ cd qb
```
3. Start the daemon process:
```sh
$ cargo run --bin qb-daemon
```
4. Start the GUI Application:
```sh
$ cargo run --bin qb-app
```

## Terminology

<h3 id="terminology-interface">interface</h3>

A QBI (quixbyte interface) is a piece of software which helps the [master](#terminology-master)
to communicate with some [storage service](#terminology-storage-service).

<h3 id="terminology-master">master</h3>

The master controls which [QBI](#terminology-interface)s to attach to and handles the communication
between the different interfaces.

<h3 id="terminology-storage-service">storage service</h3>

A storage service is an entity that we can communicate with to store and read files.

----

Project licensed under [GPLv3](LICENSE)

&copy; 2024 The QuixByte Project Authors - All Rights Reserved
