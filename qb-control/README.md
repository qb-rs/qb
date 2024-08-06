## qb-control

This crate defines the messaging protocol used to
control a quixbyte daemon process, as well as the id
implementation for tasks controlling the daemon.

It is by design dependency free (depends on qb-core and qb-proto,
which are both relatively general purpose), to allow it being used
in both the daemon and the client implementation.

----

&copy; 2024 The QuixByte Project Authors
