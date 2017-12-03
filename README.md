rget
====

[![Version](https://img.shields.io/crates/v/rget.svg)](https://crates.io/crates/rget)
[![License](http://img.shields.io/badge/license-MPL%20v2.0-blue.svg)](LICENSE)
[![Build Status](https://api.travis-ci.org/Arcterus/rget.svg?branch=master)](https://travis-ci.org/Arcterus/rget)
[![Build status](https://ci.appveyor.com/api/projects/status/uj0a67ar148kvrau?svg=true)](https://ci.appveyor.com/project/Arcterus/rget)

This program is a download accelerator primarily inspired by
[huydx/hget](https://github.com/huydx/hget).  Essentially, I was bored one
night and now here we are.  Rget is designed to work on both Windows
and Unix-like platforms.

Features
--------

* [x] Downloads remote files using HTTP and HTTPS
* [x] Downloads files using FTP and FTPS
* [ ] Downloads files using rsync
* [x] Saves incomplete downloads to be resumed later
* [ ] Verifies the integrity of file downloads
* [x] Uses multiple connections to potentially speed up downloads
* [x] Displays download progress using a progress bar
* [ ] Displays text in the user's native language

In addition to incomplete features, because rget is in very early stages of
development, there will likely be bugs.  If you encounter any please create an
issue.  If you have time, maybe you could even submit a pull request fixing the
problem. ;)

Requirements
------------

* [Rust](https://www.rust-lang.org) (>= 1.18.0)

Library Usage
-------------

Add the following to your `Cargo.toml`:
```toml
[dependencies]
rget = "0.4"
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
