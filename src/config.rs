// This file is part of rget.
//
// Copyright (C) 2017 Arcterus (Alex Lyon) and rget contributors.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use reqwest::Url;
use serde::de::{self, Deserializer, Visitor};
use serde::ser::Serializer;
use toml;

use error::RgetError;

#[derive(Default, Clone)]
pub struct Config {
   pub username: Option<String>,
   pub password: Option<String>,
   pub insecure: bool,

   pub parallel: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DownloadConfig {
   #[serde(skip)]
   pub(crate) path: PathBuf,

   #[serde(default = "parallel_default")]
   pub parallel: u64,

   #[serde(serialize_with = "serialize_url")]
   #[serde(deserialize_with = "deserialize_url")]
   pub url: Url,
}

impl DownloadConfig {
   pub fn load<P: AsRef<Path>>(path: P) -> Result<DownloadConfig, RgetError> {
      let mut data = String::new();
      File::open(&path)?.read_to_string(&mut data)?;

      Ok(toml::from_str::<DownloadConfig>(&data)?)
   }

   pub fn create(&self) -> Result<(), RgetError> {
      Ok(File::create(&self.path)?.write_all(toml::to_string(self)?.as_bytes())?)
   }

   pub fn delete(&self) -> Result<(), RgetError> {
      Ok(fs::remove_file(&self.path)?)
   }
}

struct UrlVisitor;

impl<'de> Visitor<'de> for UrlVisitor {
   type Value = Url;

   fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
      formatter.write_str("a string that represents a URL")
   }

   fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
   where
      E: de::Error,
   {
      Url::from_str(value).map_err(E::custom)
   }
}

fn serialize_url<S>(url: &Url, ser: S) -> Result<S::Ok, S::Error>
where
   S: Serializer,
{
   ser.serialize_str(url.as_str())
}

fn deserialize_url<'de, D>(de: D) -> Result<Url, D::Error>
where
   D: Deserializer<'de>,
{
   de.deserialize_str(UrlVisitor)
}

fn parallel_default() -> u64 {
   1
}
