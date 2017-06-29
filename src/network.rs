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
use std::fs::{self, File, OpenOptions};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::io::{self, Read, Write};
use std::thread;
use std::sync::Arc;
use std::time::Duration;
use std::mem;
use term::{self, StdoutTerminal, StderrTerminal};
use pbr::{MultiBar, ProgressBar, Units};
use toml::{self, Value};

use partial::FilePart;
use util;

const PRINT_DELAY: u64 = 100;

pub struct Downloader {
   parallel: u64,
   stdout: Box<StdoutTerminal>,
   stderr: Box<StderrTerminal>
}

impl Downloader {
   pub fn new(parallel: u64) -> Downloader {
      Downloader {
         parallel: parallel,
         stdout: term::stdout().unwrap(),
         stderr: term::stderr().unwrap()
      }
   }

   pub fn download(&mut self, input: &str, output: Option<&str>) -> Result<(), String> {
      let (output_path, url) = match Url::parse(input) {
         Ok(ref url) if url.scheme() != "file" => {
            // FIXME: still won't work if last character in url is /
            let closure = || input.rsplit('/').next().unwrap();
            (Path::new(output.unwrap_or_else(closure)), Some(url.clone()))
         }
         _ => {
            let closure = || input.trim_left_matches("file://").trim_right_matches(".toml");
            (Path::new(output.unwrap_or_else(closure)), None)
         }
      };
      let (parallel, url, scratch) = try!(self.reload_state(output_path, url));
      self.download_url(url, output_path, parallel, scratch)
   }

   fn download_url<P: AsRef<Path>>(&mut self,
                                   url: Url,
                                   output: P,
                                   mut parallel: u64,
                                   mut scratch: bool) -> Result<(), String> {
      // Apparently Client contains a connection pool, so reuse the same Client
      let client = Arc::new(Client::new().unwrap());

      let length = match self.get_length(client.clone(), url.clone()) {
         Some(length) => {
            self.info(&format!("remote file size: {} bytes", length));
            Some(length)
         }
         None => {
            self.warn("could not determine length of file, disabling parallel download");
            self.warn("remote file size: unknown");
            scratch = true;
            parallel = 1;
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
            Downloader::download_callback(i,
                                          progbar,
                                          client,
                                          url,
                                          output,
                                          length,
                                          parallel,
                                          scratch)
         }));
      }

      if scratch {
         if let Err(f) = self.create_download_config(output.as_ref(), url, parallel) {
            self.error(&f);  // continue, but let the user know that they can't stop the download
         }
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
         let result = self.merge_parts(parallel, output.as_ref());
         self.info("finished merging");
         match result {
            Ok(()) => self.delete_download_config(output),
            err => err
         }
      }
   }

   fn download_callback<T: Write>(part: u64,
                                  mut pb: ProgressBar<T>,
                                  client: Arc<Client>,
                                  url: Url,
                                  output: PathBuf,
                                  length: Option<u64>,
                                  parallel: u64,
                                  scratch: bool) -> Result<(), String> {
      pb.message("Waiting  : ");
      let (mut file, filelen) = if scratch {
         (FilePart::create(&output, part), 0)
      } else {
         let file = FilePart::load_or_create(&output, part);
         let len = match file.metadata() {
            Ok(data) => data.len(),
            Err(/*f*/_) => {
               //self.error(&format!("{}", f));
               //self.warn("downloading from byte 0");
               0
            }
         };
         (file, len)
      };
      let mut request = client.get(url);
      if let Some(length) = length {
         let section = length / parallel;
         if section == filelen || (part + 1 == parallel && length - section * part == filelen) {
            // FIXME: does not print correctly when the program is restarted after an interrupted
            //        download
            pb.finish_print(&format!("Completed: {}.part{}", output.display(), part));
            return Ok(());
         }
         let from = filelen + part * section;
         let to = if part + 1 == parallel {
            length
         } else {
            (part + 1) * section
         } - 1;
         request = request.header(Range::Bytes(vec![ByteRangeSpec::FromTo(from, to)]));
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
                                  output_path: P) -> Result<(), String> {
      let file = match OpenOptions::new().write(true)
                                         .create(true)
                                         .open(output_path.as_ref()) {
         Ok(m) => m,
         Err(f) => return Err(format!("{}", f))
      };
      let mut output = BufWriter::new(file);
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

   fn reload_state<P: AsRef<Path>>(&mut self,
                                   output_path: P,
                                   url: Option<Url>) -> Result<(u64, Url, bool), String> {
      match File::open(util::add_path_extension(output_path, "toml")) {
         Ok(mut file) => {
            let mut data = String::new();
            if let Err(f) = file.read_to_string(&mut data) {
               return Err(format!("{}", f));
            }
            match toml::from_str::<Value>(&data) {
               Ok(table) => {
                  let parallel = match table.get("parallel") {
                     Some(n) => match n.as_integer() {
                        Some(num) => num as u64,
                        None => return Err("number of parallel downloads in download \
                                            configuration must be an integer".to_string())
                     },
                     None => return Err("could not find number of parallel downloads in \
                                         configuration".to_string())
                  };
                  let url = match table.get("url") {
                     Some(url) => match url.as_str() {
                        Some(url_str) => match Url::parse(url_str) {
                           Ok(url) => url,
                           Err(f) => return Err(format!("{}", f))
                        },
                        None => return Err("URL in download configuration must be a \
                                            string".to_string())
                     },
                     None => return Err("could not find URL for download in \
                                         configuration".to_string())
                  };
                  Ok((parallel, url, false))
               }
               Err(f) => Err(format!("invalid data in download configuration: {:?}", f))
            }
         }
         Err(ref f) if f.kind() == io::ErrorKind::NotFound => {
            match url {
               Some(url) => Ok((self.parallel, url, true)),
               None => Err("no download configuration found and no valid URL given".to_string())
            }
         }
         Err(f) => Err(format!("{}", f))
      }
   }

   fn create_download_config<P: AsRef<Path>>(&self,
                                             output: P,
                                             url: Url,
                                             parallel: u64) -> Result<(), String> {
      #[derive(Serialize)]
      struct DownloadConfig {
         url: String,
         parallel: u64
      }
      let config = DownloadConfig {
         url: url.to_string(),
         parallel: parallel
      };
      match File::create(util::add_path_extension(output, "toml")) {
         Ok(mut file) => {
            file.write_all(toml::to_string(&config).unwrap().as_bytes()).unwrap();
            Ok(())
         }
         Err(f) => Err(format!("{}", f))
      }
   }

   fn delete_download_config<P: AsRef<Path>>(&self, output: P) -> Result<(), String> {
      match fs::remove_file(util::add_path_extension(output, "toml")) {
         Ok(()) => Ok(()),
         Err(f) => Err(format!("{}", f))
      }
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
