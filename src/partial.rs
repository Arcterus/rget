// This file is part of rget.
//
// Copyright (C) 2016 Arcterus (Alex Lyon) and rget contributors.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::fs::{self, File};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::io::{self, Write, Read};
//use std::io::{BufReader, BufWriter};

pub struct FilePart {
   file: File,
   path: PathBuf
}

impl FilePart {
   pub fn create<P: AsRef<Path>>(output: P, num: u64) -> FilePart {
      let path = FilePart::add_part_extension(output, num);
      FilePart {
         file: File::create(&path).unwrap(),
         path: path
      }
   }

   pub fn open<P: AsRef<Path>>(input: P, num: u64) -> FilePart {
      let path = FilePart::add_part_extension(input, num);
      FilePart {
         file: File::open(&path).unwrap(),
         path: path
      }
   }

   pub fn delete(self) {
      drop(self.file);
      fs::remove_file(self.path).unwrap();
   }

   fn add_part_extension<P: AsRef<Path>>(path: P, num: u64) -> PathBuf {
      let mut file_ext = path.as_ref().extension().unwrap_or(OsStr::new("")).to_os_string();
      file_ext.push(OsStr::new(&(".part".to_string() + &num.to_string())));
      path.as_ref().with_extension(file_ext)
   }
}

impl Write for FilePart {
   fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
      self.file.write(buf)
   }

   fn flush(&mut self) -> io::Result<()> {
      self.file.flush()
   }
}

impl Read for FilePart {
   fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
      self.file.read(buf)
   }
}