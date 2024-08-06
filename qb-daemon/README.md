## qb-daemon

This binary starts a daemon that manages a master
and listens on a socket for a controlling task, which
its instructions it then further processes, allowing
the controlling task to manage the daemon over IPC.

This binary is a daemon, meaning it should run somewhere
in the background and only should be interacted by the user
over other tools, which use the socket to control the daemon.

----

&copy; 2024 The QuixByte Project Authors
