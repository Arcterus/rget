rget
====

[![Version](https://img.shields.io/crates/v/rget.svg)](https://crates.io/crates/rget)
[![License](http://img.shields.io/badge/license-MPL%20v2.0-blue.svg)](LICENSE)
[![Build Status](https://api.travis-ci.org/Arcterus/rget.svg?branch=master)](https://travis-ci.org/Arcterus/rget)
[![Build status](https://ci.appveyor.com/api/projects/status/uj0a67ar148kvrau?svg=true)](https://ci.appveyor.com/project/Arcterus/rget)

This program is a download accelerator primarily inspired by
[huydx/hget](https://github.com/huydx/hget).  Essentially, I was bored one
night and now here we are.  Barring any bugs, rget should work on both Windows
and Unix-like platforms.

Features
--------

* [x] Downloads remote files using HTTP and HTTPS
* [ ] Downloads files using FTP
* [ ] Downloads files using Rsync
* [x] Saves incomplete downloads to be resumed later
* [ ] Verifies the integrity of file downloads
* [x] Uses multiple connections to potentially speed up downloads
* [x] Displays download progress using a progress bar

In addition to incomplete features, because rget is in very early stages of
development, there will likely be bugs.  If you encounter any please let me
know or submit a pull request squashing the bug so we can all download files
worry-free.

Requirements
------------

* A post-1.0 version of [Rust](https://www.rust-lang.org) (not sure which is the
oldest that will work)

Library Usage
-------------

Add the following to your `Cargo.toml`:
```toml
[dependencies]
rget = "0.3"
```

Build
-----

```bash
$ git clone https://github.com/Arcterus/rget
$ cd rget
$ cargo build
```

Install
-------

For the bleeding edge version:
```bash
$ cargo install https://github.com/Arcterus/rget
```

For the latest stable version:
```bash
$ cargo install rget
```

License
-------

rget is licensed under the MPL v2.0.  See [LICENSE](LICENSE) for more details.
