# How to contribute

Welcome stranger, thanks for being here.

0. Take a look at the contribution guidelines.

1. [Create a fork](https://github.com/qb-rs/qb/fork) of this repository.

2. Clone the forked repository locally.
```sh
$ git clone https://github.com/YourUsername/qb
```
3. The fun part: Make changes.

4. Test your changes.

You can find detailed instructions on how to do this
in the packages you are testing, but here is a quick example:
```sh
# Spawn a daemon
$ cargo run --bin qb-daemon -- -p run/daemon1
# Add an interface
$ cargo run --bin qb-cli -- start local '{"path":"run/local1"}'
```

5. Commit you changes.

Always write a clear log message for your commits. One-line messages are fine
for small changes, but bigger changes should look like this:

```sh
$ git commit -m "A brief summary of the commit
> 
> A paragraph describing what changed and its impact."
```

6. [Create a pull request](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/proposing-changes-to-your-work-with-pull-requests/creating-a-pull-request) with a detailed description of what you did and why.

## Note on Licensing

By contributing to this repository you acknowledge that all contributed code must
be compatible with our [LICENSE](../LICENSE). You acknowledge that you also must
have the right to contribute the changes, while respecting copyright, local and global laws.


## Naming conventions

- A package is a directory that contains stuff related to the QuixByte Application.
This excludes documentation, website and CI related files.
- Packages should start with the `qb-` prefix if they ...
1. ... host a core binary used by the Application Stack.
2. ... host a core library used by Application Stack.
3. ... provide an Application Client for certain platform(s). <!--Maybe-->
- Packages should start with the `qb-ext-` prefix if they ...
1. ... provide a hook for extending QuixByte's functionality.
2. ... provide an interface for extending QuixByte's functionality.
- If a package provides an Application Client that is platform 
specific it should be named `qb-<platform (group)>`. (qb-mobile, qb-desktop, qb-web, qb-android, qb-ios, ...)

- Structures should start with the `QBI` prefix if they ...
1. ... implement the QBIContext trait.
2. ... specifically are helper structures for another structure, which implements the QBIContext trait.
- Structures should start with the `QBH` prefix if they ...
1. ... implement the QBHContext trait.
2. ... specifically are helper structures for another structure, which implements the QBHContext trait.
- Structures should end with the `Setup` suffix if they ...
1. ... implement the QBExtSetup trait.

----

&copy; 2024 The QuixByte Project Authors - All Rights Reserved
