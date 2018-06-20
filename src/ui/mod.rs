use std::io::{self, Write};
use std::path::PathBuf;

pub mod multibar;

pub trait Interface: Send {
   type Part: PartInterface;

   fn init(output: PathBuf) -> Self;

   // FIXME: this is wrong afaict (the type should always be the same for an impl)
   fn part_interface(&mut self) -> Self::Part;

   fn listen(&mut self);
}

pub trait PartInterface: Send {
   fn restore(&mut self, total_length: u64, content_length: u64);
   fn update(&mut self, new_progress: usize);
   fn complete(&mut self);
   fn fail(&mut self);
}

pub struct PartWriter<'a, I: 'a + PartInterface> {
   interface: &'a mut I,
}

impl<'a, I: 'a + PartInterface> PartWriter<'a, I> {
   pub fn new(interface: &'a mut I) -> PartWriter<'a, I> {
      PartWriter {
         interface: interface,
      }
   }
}

impl<'a, I: 'a + PartInterface> Write for PartWriter<'a, I> {
   fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
      self.interface.update(buf.len());

      Ok(buf.len())
   }

   fn flush(&mut self) -> io::Result<()> {
      Ok(())
   }
}
