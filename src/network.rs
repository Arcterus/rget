// This file is part of rget.
//
// Copyright (C) 2016 Arcterus (Alex Lyon) and rget contributors.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use reqwest::{Client, Url};
use reqwest::StatusCode;
use reqwest::header::{ContentLength, /*ContentRange,*/ Range, ByteRangeSpec};
use std::u64;
use std::fs::{OpenOptions, File};
use std::io::{Seek, SeekFrom, BufWriter};
use std::path::{Path, PathBuf};
use std::io::{self, Read, Write};
use std::thread;
use std::sync::Arc;
use std::time::Duration;
use std::mem;
use term::{self, StdoutTerminal, StderrTerminal};
use pbr::{MultiBar, ProgressBar, Units};

use partial::FilePart;

const PRINT_DELAY: u64 = 100;

pub struct Downloader {
   parallel: u64,
   size: Option<u64>,
   stdout: Box<StdoutTerminal>,
   stderr: Box<StderrTerminal>
}

impl Downloader {
   pub fn new(parallel: u64) -> Downloader {
      Downloader {
         parallel: parallel,
         size: None,
         stdout: term::stdout().unwrap(),
         stderr: term::stderr().unwrap()
      }
   }

   pub fn download(&mut self, input: &str, output: Option<&str>) -> Result<(), String> {
      match Url::parse(input) {
         // FIXME: still won't work if last character in url is /
         Ok(url) => {
            let closure = || input.rsplit('/').next().unwrap();
            self.download_url(url, Path::new(output.unwrap_or_else(closure)))
         }
         Err(_) => {
            let closure = || input.trim_right_matches(".toml");
            self.download_file(input, Path::new(output.unwrap_or_else(closure)))
         }
      }
   }

   fn download_file<P: AsRef<Path>>(&mut self, filename: &str, output: P) -> Result<(), String> {
      let _ = filename;    // to shut the compiler up for now
      let _ = output;
      unimplemented!()
   }

   fn download_url<P: AsRef<Path>>(&mut self, url: Url, output: P) -> Result<(), String> {
      let file = match OpenOptions::new().read(true)
                                         .write(true)
                                         .create(true)
                                         .open(output.as_ref()) {
         Ok(m) => m,
         Err(f) => return Err(format!("{}", f))
      };
      let mut file = BufWriter::new(file);

      let mut bytes = match file.seek(SeekFrom::End(0)) {
         Ok(m) => m,
         Err(f) => {
            self.error(&format!("{}", f));
            self.warn("Downloading from byte 0");
            0
         }
      };

      // Apparently Client contains a connection pool, so reuse the same Client
      let client = Arc::new(Client::new().unwrap());

      let mut parallel = self.parallel;
      let length = match self.get_length(client.clone(), url.clone()) {
         Some(length) => {
            if length == bytes {
               self.warn("file already downloaded");
               return Ok(());
            } else if bytes > length {
               self.warn("corrupted file, redownloading");
               if let Err(f) = file.seek(SeekFrom::Start(0)) {
                  return Err(format!("{}", f));
               }
               bytes = 0;
            }

            let size = length - bytes;
            self.size = Some(size);
            self.info(&format!("download size: {} bytes", size));
            Some(length)
         }
         None => {
            self.warn("could not determine length of file, disabling parallel download");
            self.warn("download size: unknown");
            parallel = 1;
            self.size = None;
            None
         }
      };

      self.info(&format!("using a total of {} connections", parallel));

      let mut children = vec![];
      let mut mb = MultiBar::new();

      for i in 0u64..parallel {
         let url = url.clone();
         let output = output.as_ref().to_path_buf();
         let client = client.clone();
         let mut progbar = mb.create_bar(100);

         progbar.set_max_refresh_rate(Some(Duration::from_millis(PRINT_DELAY)));
         progbar.show_message = true;
         progbar.set_units(Units::Bytes);

         children.push(thread::spawn(move || {
            Downloader::download_callback(i, progbar, client, url, output, length, bytes, parallel)
         }));
      }

      mb.listen();

      let mut result = "".to_string();
      for child in children {
         match child.join() {
            Ok(Err(f)) => result += &format!("{}\n", f),
            Err(f) => result += &format!("{:?}\n", f),
            _ => {}
         }
      }

      if result.len() > 0 {
         Err(result.trim_right().to_string())
      } else {
         self.info("merging parts... ");
         let result = self.merge_parts(parallel, file, output);
         self.info("finished merging");
         result
      }
   }

   fn download_callback<T: Write>(part: u64,
                                  mut pb: ProgressBar<T>,
                                  client: Arc<Client>,
                                  url: Url,
                                  output: PathBuf,
                                  length: Option<u64>,
                                  bytes: u64,
                                  parallel: u64) -> Result<(), String> {
      pb.message("Waiting  : ");
      let mut request = client.get(url);
      if let Some(length) = length {
         let section = (length - bytes) / parallel;
         let from = bytes + part * section;
         request = request.header(Range::Bytes(vec![ByteRangeSpec::FromTo(from,
                                                                          if part + 1 == parallel {
                                                                             length
                                                                          } else {
                                                                             from + section
                                                                          } - 1)]));
      }
      let part = part as usize;
      let result = match request.send() {
         Ok(mut resp) => {
            pb.message("Connected: ");
            // FIXME: is this right/all?
            if resp.status() == &StatusCode::Ok || resp.status() == &StatusCode::PartialContent {
               let &ContentLength(length) = resp.headers().get()
                                                          .unwrap_or(&ContentLength(u64::MAX));
               pb.total = length;
               // TODO: check accept-ranges or whatever
               let mut file = FilePart::create(&output, part as u64);
               let mut buffer: [u8; 8192] = unsafe { mem::uninitialized() };
               let mut downloaded = 0;
               while downloaded < length {
                  match resp.read(&mut buffer) {
                     Ok(n) => {
                        if n == 0 {
                           break;
                        } else {
                           downloaded += n as u64;
                           file.write_all(&buffer[0..n]).unwrap();
                           pb.add(n as u64);
                        }
                     }
                     Err(f) => return Err(format!("{}", f))
                  }
                  pb.tick();
               }
               pb.finish_print(&format!("Completed: {}.part{}", output.display(), part));
               Ok(())
            } else {
               pb.finish_print(&format!("Failed   : {}.part{}", output.display(), part));
               Err(format!("received {} from server", resp.status()))
            }
         }
         Err(f) => {
            pb.finish_print(&format!("Failed   : {}.part{}", output.display(), part));
            Err(format!("{}", f))
         }
      };
      result
   }

   fn merge_parts<P: AsRef<Path>>(&self,
                                  parallel: u64,
                                  mut output: BufWriter<File>,
                                  output_path: P) -> Result<(), String> {
      let mut total_size = 0;
      for i in 0..parallel {
         let mut infile = FilePart::open(output_path.as_ref(), i);
         match io::copy(&mut infile, &mut output) {
            Ok(n) => total_size += n as u64,
            Err(f) => return Err(format!("{}", f))
         }
         infile.delete();
      }
      output.into_inner().unwrap().set_len(total_size).unwrap();
      Ok(())
   }

   fn get_length(&self, client: Arc<Client>, url: Url) -> Option<u64> {
      match client.get(url).send() {
         Ok(resp) => {
            if resp.status() == &StatusCode::Ok {
               match resp.headers().get() {
                  Some(&ContentLength(length)) => Some(length),
                  None => None
               }
            } else {
               None
            }
         }
         Err(_) => None
      }
   }

   fn info(&mut self, msg: &str) {
      self.stdout.fg(term::color::GREEN).unwrap();
      writeln!(self.stdout, "info: {}", msg).unwrap();
      self.stdout.reset().unwrap();
   }

   fn warn(&mut self, msg: &str) {
      self.stdout.fg(term::color::YELLOW).unwrap();
      writeln!(self.stdout, "warn: {}", msg).unwrap();
      self.stdout.reset().unwrap();
   }

   fn error(&mut self, msg: &str) {
      self.stderr.fg(term::color::RED).unwrap();
      writeln!(self.stderr, "error: {}", msg).unwrap();
      self.stderr.reset().unwrap();
   }
}
