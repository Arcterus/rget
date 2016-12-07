use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub fn add_path_extension<P: AsRef<Path>>(path: P, ext: &str) -> PathBuf {
   let mut file_ext = path.as_ref().extension().unwrap_or(OsStr::new("")).to_os_string();
   file_ext.push(OsStr::new(&format!(".{}", ext)));
   path.as_ref().with_extension(file_ext)
}