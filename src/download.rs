// This file is part of rget.
//
// Copyright (C) 2017 Arcterus (Alex Lyon) and rget contributors.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use reqwest::{Client, RequestBuilder, Url};
use reqwest::StatusCode;
use reqwest::header::{
   Authorization,
   Basic,
   ByteRangeSpec,
   ContentLength,
   /*ContentRange,*/
   Range,
};
use std::u64;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::io::{self, BufWriter};
use std::thread;
use std::sync::Arc;
use std::borrow::Borrow;
use std::convert::Into;
use number_prefix::{decimal_prefix, Standalone, Prefixed};
use broadcast::BroadcastWriter;

use network::Rget;
use config::{Config, DownloadConfig};
use partial::FilePart;
use util;
use error::RgetError;
use output::OutputManager;
use ui::{Interface, PartInterface, PartWriter};

pub trait Downloader {
   fn download<I: Interface + 'static, P: AsRef<Path>>(&mut self, input: &str, output: Option<P>) -> Result<(), RgetError>;
}

impl<T: OutputManager> Downloader for Rget<T> {
   fn download<I: Interface + 'static, P: AsRef<Path>>(&mut self, input: &str, output: Option<P>) -> Result<(), RgetError> {
      let (output_path, url) = match Url::parse(input) {
         Ok(ref url) if url.scheme() != "file" => {
            // FIXME: still won't work if last character in url is /
            let closure = || Path::new(input.rsplit('/').next().unwrap());
            (output.as_ref().map(AsRef::as_ref).unwrap_or_else(closure), Some(url.clone()))
         }
         _ => {
            let closure = || Path::new(input.trim_left_matches("file://").trim_right_matches(".toml"));
            (output.as_ref().map(AsRef::as_ref).unwrap_or_else(closure), None)
         }
      };

      let (config, scratch) = self.reload_state(output_path, url)?;
      self.download_url::<I>(config, output_path, scratch)
   }
}

/// A private trait so less state needs to be explicitly passed to each method.
trait DownloaderImpl {
   fn download_url<I: Interface + 'static>(&mut self,
                   config: DownloadConfig,
                   output: &Path,
                   scratch: bool) -> Result<(), RgetError>;
   // TODO: this may be better as part of Rget (so it can be reused for other impls)
   fn merge_parts(&self,
                  parallel: u64,
                  output_path: &Path) -> Result<(), RgetError>;
   // TODO: this may be better as part of Rget (so it can be reused for other impls)
   fn reload_state(&mut self,
                   output_path: &Path,
                   given_url: Option<Url>) -> Result<(DownloadConfig, bool), RgetError>;
   fn get_length(&self, client: Arc<Client>, url: Url) -> Option<u64>;
}

impl<T: OutputManager> DownloaderImpl for Rget<T> {
   fn download_url<I: Interface + 'static>(&mut self,
                   mut config: DownloadConfig,
                   output: &Path,
                   mut scratch: bool) -> Result<(), RgetError> {
      // Apparently Client contains a connection pool, so reuse the same Client
      let mut client_builder = Client::builder();
      if self.config.insecure {
         client_builder.danger_disable_hostname_verification();
      }
      let client = Arc::new(client_builder.build()?);

      let length = match self.get_length(client.clone(), config.url.clone()) {
         Some(length) => {
            let (disp_len, units) = match decimal_prefix(length as f64) {
               Standalone(bytes) => (bytes, "bytes".to_string()),
               Prefixed(prefix, n) => (n, format!("{}B", prefix))
            };
            self.output.info(&format!("remote file size: {} {}", disp_len, units));
            Some(length)
         }
         None => {
            self.output.warn("could not determine length of file, disabling parallel download");
            self.output.warn("remote file size: unknown");
            scratch = true;
            config.parallel = 1;
            None
         }
      };

      self.output.info(&format!("using a total of {} connections", config.parallel));

      let mut children = vec![];
      let mut interface = I::init(output.to_path_buf());

      for i in 0u64..config.parallel {
         let download_config = config.clone();
         let client = client.clone();
         let output = output.to_path_buf();
         let downloader_config = self.config.clone();

         let interface = interface.part_interface();

         children.push(thread::spawn(move || {
            download_callback(i,
                              interface,
                              client,
                              download_config,
                              output,
                              length,
                              downloader_config,
                              scratch)
         }));
      }

      if scratch {
         if let Err(f) = config.create() {
            self.output.error(&format!("{}", f));  // continue, but let the user know that they can't stop the download
         }
      }

      thread::spawn(move || {
         interface.listen();
      });

      let errors = children.into_iter().filter_map(|child| {
         match child.join() {
            Ok(Err(f)) => Some(f),
            Err(f) => Some(RgetError::FailedThread(format!("{:?}", f))),
            _ => None
         }
      }).collect::<Vec<_>>();

      if errors.len() > 0 {
         Err(RgetError::Multiple(errors))
      } else {
         self.output.info("merging parts... ");
         self.merge_parts(config.parallel, output.borrow())?;
         self.output.info("finished merging");
         config.delete()
      }
   }

   fn merge_parts(&self,
                  parallel: u64,
                  output_path: &Path) -> Result<(), RgetError> {
      let file = OpenOptions::new().write(true)
                                   .create(true)
                                   .open(&output_path)?;

      let mut output = BufWriter::new(file);
      let mut total_size = 0;
      for i in 0..parallel {
         let mut infile = FilePart::open(&output_path, i);
         total_size += io::copy(&mut infile, &mut output)? as u64;
         infile.delete();
      }
      output.into_inner().unwrap().set_len(total_size)?;

      Ok(())
   }

   fn reload_state(&mut self,
                   output_path: &Path,
                   given_url: Option<Url>) -> Result<(DownloadConfig, bool), RgetError> {
      let path = util::add_path_extension(output_path, "toml");
      match DownloadConfig::load(&path) {
         Ok(mut config) => {
            if let Some(url) = given_url {
               config.url = url;
            }
            config.path = path;

            Ok((config, false))
         },

         Err(RgetError::Io(ref f)) if f.kind() == io::ErrorKind::NotFound => {
            match given_url {
               Some(url) => {
                  let config = DownloadConfig {
                     path: path,

                     url: url,
                     parallel: self.config.parallel
                  };
                  Ok((config, true))
               }
               None => Err(RgetError::MissingUrl)
            }
         },

         Err(f) => Err(f.into())
      }
   }

   fn get_length(&self, client: Arc<Client>, url: Url) -> Option<u64> {
      let mut request = client.get(url);
      if let Some(ref username) = self.config.username {
         request.header(Authorization(Basic {
            username: username.to_owned(),
            password: self.config.password.to_owned(),
         }));
      }

      // convert the error into an option because the download can still
      // continue if we are unable to get the length (we just have to switch
      // to downloading using a single thread rather than in parallel)
      request.send().ok().and_then(|resp| {
         if resp.status() == StatusCode::Ok {
            resp.headers().get().map(|&ContentLength(length)| length)
         } else {
            None
         }
      })
   }
}

fn download_callback<I: PartInterface>(part: u64,
                               mut interface: I,
                               client: Arc<Client>,
                               download_config: DownloadConfig,
                               output: PathBuf,
                               length: Option<u64>,
                               config: Config,
                               scratch: bool) -> Result<(), RgetError> {
   let (file, filelen) = if scratch {
      (FilePart::create(&output, part), 0)
   } else {
      let file = FilePart::load_or_create(&output, part);
      let len = match file.metadata() {
         Ok(data) => data.len(),
         Err(/*f*/_) => {
            //self.output.error(&format!("{}", f));
            //self.output.warn("downloading from byte 0");
            0
         }
      };
      (file, len)
   };

   let mut request = client.get(download_config.url.clone());
   if set_download_range(part, &mut interface, &mut request, &download_config, length, filelen) {
      return Ok(())
   }

   if let Some(username) = config.username {
      request.header(Authorization(Basic {
         username: username,
         password: config.password,
      }));
   }

   request.send().map_err(Into::into).and_then(|mut resp| {
      // FIXME: is this right/all?
      if resp.status() == StatusCode::Ok || resp.status() == StatusCode::PartialContent {
         let &ContentLength(content_length) = resp.headers().get()
                                                            .unwrap_or(&ContentLength(u64::MAX));
         let parallel = download_config.parallel;
         let mut length = length.map(|len| len / parallel).unwrap_or(content_length);

         if length < content_length {
            // the last part may be slightly longer due to integer truncation
            length = content_length;
         }
         interface.restore(length, content_length);
         // TODO: check accept-ranges or whatever

         io::copy(&mut resp, &mut BufWriter::new(BroadcastWriter::new(file, PartWriter::new(&mut interface))))?;

         interface.complete();
         Ok(())
      } else {
         Err(resp.status().into())
      }
   }).map_err(|err| {
      interface.fail();
      err
   })
}

// NOTE: returns whether download is already done
fn set_download_range<I: PartInterface>(part: u64,
                                interface: &mut I,
                                request: &mut RequestBuilder,
                                config: &DownloadConfig,
                                length: Option<u64>,
                                filelen: u64) -> bool {
   if let Some(length) = length {
      let section = length / config.parallel;
      if section == filelen || (part + 1 == config.parallel && length - section * part == filelen) {
         interface.complete();
         return true;
      }
      let from = filelen + part * section;
      let to = if part + 1 == config.parallel {
         length
      } else {
         (part + 1) * section
      } - 1;
      request.header(Range::Bytes(vec![ByteRangeSpec::FromTo(from, to)]));
   }

   false
}
