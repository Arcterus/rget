pub mod ftp;
pub mod http;

pub trait ProtocolClient {}

pub trait Protocol {
   fn get_file_length() -> Option<u64>;
}
