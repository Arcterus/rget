// This file is part of rget.
//
// Copyright (C) 2016-2017 Arcterus (Alex Lyon) and rget contributors.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use reqwest::{self, StatusCode};
use std::io;
use toml;

use failure::Error;

pub type Result<T> = ::std::result::Result<T, Error>;

#[derive(Debug, Fail)]
pub enum RgetError {
   #[fail(display = "{}", _0)]
   Io(#[cause] io::Error),

   #[fail(display = "no download configuration found and no valid URL given")]
   MissingUrl,

   #[fail(display = "received {} from server", _0)]
   HttpErrorCode(StatusCode),

   #[fail(display = "{}", _0)]
   FailedRequest(#[cause] reqwest::Error),

   #[fail(display = "{}", _0)]
   InvalidConfig(&'static str),

   #[fail(display = "invalid data in download configuration: {:?}", _0)]
   InvalidToml(#[cause] toml::de::Error),

   #[fail(display = "invalid data in download configuration: {:?}", _0)]
   InvalidDownloadConfig(#[cause] toml::ser::Error),

   #[fail(display = "{}", _0)]
   InvalidUrl(#[cause] reqwest::UrlError),

   #[fail(display = "{}", _0)]
   FailedThread(String),

   #[fail(display = "{:?}", _0)]
   Multiple(Vec<RgetError>),
}

macro_rules! error_impl {
   ($typ:ty, $rget:expr) => {
      impl From<$typ> for RgetError {
         fn from(err: $typ) -> Self {
            $rget(err)
         }
      }
   };
}

error_impl!(io::Error, RgetError::Io);
error_impl!(StatusCode, RgetError::HttpErrorCode);
error_impl!(reqwest::Error, RgetError::FailedRequest);
error_impl!(toml::de::Error, RgetError::InvalidToml);
error_impl!(toml::ser::Error, RgetError::InvalidDownloadConfig);
error_impl!(reqwest::UrlError, RgetError::InvalidUrl);
