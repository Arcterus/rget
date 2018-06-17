// This file is part of rget.
//
// Copyright (C) 2016-2017 Arcterus (Alex Lyon) and rget contributors.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use config::Config;
use output::{OutputManager, StdOutputManager};

/// Parallel file downloader with configurable output.
///
/// Currently only supports HTTP and HTTPS, but rsync and FTP support are
/// planned.
///
/// ```
/// use rget::{Config, Downloader};
///
/// fn main() {
///    let url = "https://static.rust-lang.org/dist/rust-1.21.0-x86_64-unknown-linux-gnu.tar.gz";
///
///    // construct a default Downloader that starts four parallel downloads
///    let downloader = Downloader::new(4, Config::default());
///
///    // download the file specified by `url` to the current directory as a
///    // file called `rust-1.21.0-x86_64-unknown-linux-gnu.tar.gz`.
///    if let Err(err) = downloader.download(url, None) {
///       println!("error: ", err);
///    }
/// }
/// ```
pub struct Rget<T: OutputManager> {
   pub(crate) config: Config,
   pub(crate) output: T,
}

impl Rget<StdOutputManager> {
   pub fn new(config: Config) -> Rget<StdOutputManager> {
      Rget::with_output_manager(config, StdOutputManager::new())
   }
}

impl<T: OutputManager> Rget<T> {
   pub fn with_output_manager(config: Config, output: T) -> Rget<T> {
      Rget {
         config: config,
         output: output,
      }
   }
}
