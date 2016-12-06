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
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use std::mem;
use term_size;
use term::{self, StdoutTerminal, StderrTerminal};
use number_prefix::{self, Standalone, Prefixed};

use partial::FilePart;

const PRINT_DELAY: u64 = 500;

pub struct Downloader {
   parallel: u64,
   downloaded: u64,
   size: Option<u64>,
   stdout: Box<StdoutTerminal>,
   stderr: Box<StderrTerminal>
}

impl Downloader {
   pub fn new(parallel: u64) -> Downloader {
      Downloader {
         parallel: parallel,
         downloaded: 0,
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
      let downloaded = Arc::new(RwLock::new(vec![]));
      let size = Arc::new(RwLock::new(vec![]));
      let done = Arc::new(RwLock::new(vec![]));

      for i in 0u64..parallel {
         downloaded.write().unwrap().push(0);
         size.write().unwrap().push(0);
         done.write().unwrap().push(false);

         let downloaded = downloaded.clone();
         let size = size.clone();
         let done = done.clone();
         let url = url.clone();
         let output = output.as_ref().to_path_buf();
         let client = client.clone();

         children.push(thread::spawn(move || {
            Downloader::download_url_thread_cb(i,
                                               downloaded,
                                               size,
                                               done,
                                               client,
                                               url,
                                               output,
                                               length,
                                               bytes,
                                               parallel)
         }));
      }

      self.track_progress(done, downloaded);

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
         self.merge_parts(parallel, file, output)
      }
   }

   fn download_url_thread_cb(part: u64,
                             downloaded: Arc<RwLock<Vec<u64>>>,
                             size: Arc<RwLock<Vec<u64>>>,
                             done: Arc<RwLock<Vec<bool>>>,
                             client: Arc<Client>,
                             url: Url,
                             output: PathBuf,
                             length: Option<u64>,
                             bytes: u64,
                             parallel: u64) -> Result<(), String> {
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
            // FIXME: is this right/all?
            if resp.status() == &StatusCode::Ok || resp.status() == &StatusCode::PartialContent {
               let &ContentLength(length) = resp.headers().get()
                                                          .unwrap_or(&ContentLength(u64::MAX));
               size.write().unwrap()[part] = length;
               // TODO: check accept-ranges or whatever
               let mut file = FilePart::create(output, part as u64);
               let mut buffer: [u8; 8192] = unsafe { mem::uninitialized() };
               while downloaded.read().unwrap()[part] < size.read().unwrap()[part] {
                  match resp.read(&mut buffer) {
                     Ok(n) => {
                        if n == 0 {
                           break;
                        } else {
                           downloaded.write().unwrap()[part] += n as u64;
                           file.write_all(&buffer[0..n]).unwrap();
                        }
                     }
                     Err(f) => return Err(format!("{}", f))
                  }
               }
               Ok(())
            } else {
               Err(format!("received {} from server", resp.status()))
            }
         }
         Err(f) => Err(format!("{}", f))
      };
      done.write().unwrap()[part] = true;
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

   fn track_progress(&mut self, done: Arc<RwLock<Vec<bool>>>, downloaded: Arc<RwLock<Vec<u64>>>) {
      let mut now = Instant::now();
      while !done.read().unwrap().iter().all(|&val| val) {
         self.print_status(downloaded.clone(), &now);
         now = Instant::now();
         thread::sleep(Duration::from_millis(PRINT_DELAY));
      }
      self.print_status(downloaded.clone(), &now);
      println!("");
   }

   fn print_status(&mut self, downloaded: Arc<RwLock<Vec<u64>>>, now: &Instant) {
      let downloaded: u64 = downloaded.read().unwrap().iter().sum();

      let elapsed = now.elapsed();
      let secs = elapsed.as_secs() as f64 + (elapsed.subsec_nanos() as f64 / 1_000_000_000.);
      let speed = if secs == 0. {
         0
      } else {
         ((downloaded - self.downloaded) as f64 / secs) as u64
      };

      let (days, hours, mins, secs) = self.calculate_time(downloaded, speed);
      let remain_time = self.generate_remain_time(days, hours, mins, secs);

      // FIXME: should print something else out if self.size is None
      let percent = if self.size.unwrap_or(0) == 0 {
         0.
      } else {
         downloaded as f64 / self.size.unwrap() as f64
      };

      let (speed, prefix) = match number_prefix::binary_prefix(speed as f64) {
         Standalone(bytes) => (bytes, "".to_string()),
         Prefixed(prefix, n) => (n, prefix.to_string())
      };

      match term_size::dimensions() {
         Some((width, _)) => {
            let speed_digits = self.count_digits(speed as u64) + 2;
            let gwidth = width as u64 - self.count_digits((percent * 100.) as u64) -
                         1 /* percent + % */ - 3 /* brackets + space */ -
                         remain_time.len() as u64 /* remaining time */ - 7 /* speed + space */ -
                         speed_digits - prefix.len() as u64;
            let awidth = if percent * gwidth as f64 >= 1. {
               1
            } else {
               0
            };
            let pwidth = (percent * gwidth as f64) as u64 - awidth as u64;
            let swidth = gwidth - pwidth - 1;
            write!(self.stdout, "\r[{:=<pwidth$}{}{:swidth$}] ({:.1} {}B/s) {}{}%",
                   "", if awidth > 0 { ">" } else { "" }, "", speed, prefix, remain_time,
                   (percent * 100.) as u64, pwidth = pwidth as usize,
                   swidth = swidth as usize).unwrap();
         }
         None => write!(self.stdout, "\r{} / {} [{}%] ({:.1} {}B/sec)",
                        downloaded, self.size.and_then(|n| Some(n.to_string()))
                                             .unwrap_or("?".to_string()),
                        (percent * 100.) as u64, speed, prefix).unwrap()
      }
      io::stdout().flush().unwrap();
      self.downloaded = downloaded;
   }

   fn count_digits(&self, mut n: u64) -> u64 {
      let mut digits = 1;
      while n > 9 {
         n /= 10;
         digits += 1;
      }
      digits
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

   fn calculate_time(&self, downloaded: u64, speed: u64) -> (u64, u64, u64, u64) {
      match self.size {
         Some(size) if speed > 0 => {
            let total_secs = (size - downloaded) / speed;
            let secs = total_secs % 60;
            let total_mins = total_secs / 60;
            let mins = total_mins % 60;
            let total_hours = total_mins / 60;
            let hours = total_hours % 24;
            let total_days = total_hours / 24;
            (total_days, hours, mins, secs)
         }
         _ => (0, 0, 0, 0)
      }
   }

   fn generate_remain_time(&self, days: u64, hours: u64, mins: u64, secs: u64) -> String {
      let mut remain_time = "".to_string();
      if days > 0 {
         remain_time += &format!("{}d", days);
      }
      if hours > 0 {
         remain_time += &format!("{}h", hours);
      }
      if mins > 0 {
         remain_time += &format!("{}m", mins);
      }
      if secs > 0 {
         remain_time += &format!("{}s", secs);
      }
      if remain_time.len() > 0 {
         remain_time += " ";
      }
      remain_time
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
