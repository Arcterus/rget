use super::{Interface, PartInterface};

use std::io::Stdout;
use std::path::PathBuf;
use std::time::Duration;

use pbr::{MultiBar, ProgressBar, Units, Pipe};

const PRINT_DELAY: u64 = 100;

pub struct MultibarUi {
   output: PathBuf,
   multibar: MultiBar<Stdout>,
   part_count: u64
}

pub struct ProgressbarUi {
   part: u64,
   output: PathBuf,
   progbar: ProgressBar<Pipe>
}

impl Interface for MultibarUi {
   type Part = ProgressbarUi;

   fn init(output: PathBuf) -> MultibarUi {
      MultibarUi {
         output: output,
         multibar: MultiBar::new(),
         part_count: 0
      }
   }

   fn part_interface(&mut self) -> ProgressbarUi {
      let mut progbar = self.multibar.create_bar(100);

      progbar.set_max_refresh_rate(Some(Duration::from_millis(PRINT_DELAY)));
      progbar.show_message = true;
      progbar.set_units(Units::Bytes);

      progbar.message("Waiting  : ");
      progbar.tick();

      let subui = ProgressbarUi {
         part: self.part_count,
         output: self.output.clone(),
         progbar: progbar
      };

      self.part_count += 1;

      subui
   }

   fn listen(&mut self) {
      self.multibar.listen();
   }
}

impl PartInterface for ProgressbarUi {
   fn restore(&mut self, total_length: u64, content_length: u64) {
      self.progbar.message("Connected: ");
      self.progbar.total = total_length;
      self.progbar.add(total_length - content_length);
   }

   fn complete(&mut self) {
      self.progbar.finish_print(&format!("Completed: {}.part{}", self.output.display(), self.part));
   }

   fn update(&mut self, new_progress: usize) {
      self.progbar.add(new_progress as u64);
   }

   fn fail(&mut self) {
      self.progbar.finish_print(&format!("Failed   : {}.part{}", self.output.display(), self.part));
   }
}