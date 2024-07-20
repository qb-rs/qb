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

----

# Table of Contents

1. [Goals](#goals)
2. [Platform Support](#platform-support)
3. [QBI Support](#qbi-support)
4. [Quickstart](#quickstart)
5. [Termology](#termology)

----

# Goals

- Fast, resilient sync
- Low resource usage
- Support of many different [storage services](#termology-storage-service)
- Extensibility, allow external processes to act as [QBI](#termology-interface)s
- Wide platform support
- Entirely opensource

----

# Platform Support

<!-- TODO: tidy this -->

platform|arch|support|planned
---|---|---|---
Linux|x86_64|full, tested|yes
Linux|any|untested, should work|yes
Windows|any|currently no support|yes
Mac-OS|any|untested, should work|yes
Android|any|unknown|yes
iOS|any|unknown|yes

----

# QBI Support

<!-- TODO: tidy this -->

service|description|support|planned
---|---|---|---
qbi-local|sync to local folder|yes|yes
qbi-rtc|sync via WebRTC|unimplemented|yes
qbi-server|start server|unimplemented|yes
qbi-client|sync to server|unimplemented|yes
qbi-gdrive|sync to Google Drive|unimplemented|yes
qbi-dropbox|sync to Dropbox|unimplemented|yes

----

# Quickstart

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

----

# Termology

<h2 id="termology-interface">interface</h2>

A QBI (quixbyte interface) is a piece of software which helps the [master](#termology-master)
to communicate with some [storage service](#termology-storage-service).

<h2 id="termology-master">master</h2>

The master controls which [QBI](#termology-interface)s to attach to and handles the communication
between the different interfaces.

<h2 id="termology-storage-service">storage service</h2>

A storage service is an entity that we can communicate with to store and read files.

----

&copy; 2024 The QuixByte Project Authors - All Rights Reserved
