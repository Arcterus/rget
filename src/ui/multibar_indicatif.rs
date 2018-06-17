use super::{Interface, PartInterface};

use std::path::PathBuf;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

#[derive(Clone)]
pub struct UiStyle {
   pub running: ProgressStyle,
   pub finished: ProgressStyle,
   pub failed: ProgressStyle,
}

impl Default for UiStyle {
   fn default() -> Self {
      UiStyle {
         running: ProgressStyle::default_bar()
            .template(
               "[{elapsed_precise}] \
                {spinner:.yellow} \
                │{bar:40.blue}│ \
                {bytes}/{total_bytes} \
                ({eta}) \
                {msg:.blue} ",
            )
            .progress_chars("█▛▌▖  "),
         finished: ProgressStyle::default_bar().template(
            "[{elapsed_precise}] \
             {msg:.green}",
         ),
         failed: ProgressStyle::default_bar().template(
            "[{elapsed_precise}] \
             {msg:.red}",
         ),
      }
   }
}

pub struct MultibarUi {
   output: PathBuf,
   multibar: MultiProgress,
   part_count: u64,
   style: UiStyle,
}

pub struct ProgressbarUi {
   part: u64,
   output: PathBuf,
   progbar: ProgressBar,
   style: UiStyle,
}

impl Interface for MultibarUi {
   type Part = ProgressbarUi;

   fn init(output: PathBuf) -> MultibarUi {
      MultibarUi {
         output: output,
         multibar: MultiProgress::new(),
         part_count: 0,
         style: UiStyle::default(),
      }
   }

   fn part_interface(&mut self) -> ProgressbarUi {
      let progbar = self.multibar.add(ProgressBar::new(100));
      progbar.set_style(self.style.running.clone());

      progbar.set_message("Waiting  : ");
      progbar.tick();

      let subui = ProgressbarUi {
         part: self.part_count,
         output: self.output.clone(),
         progbar: progbar,
         style: self.style.clone(),
      };

      self.part_count += 1;

      subui
   }

   fn listen(&mut self) {
      self.multibar.join().expect("Could not join multibar");
   }
}

impl PartInterface for ProgressbarUi {
   fn restore(&mut self, total_length: u64, content_length: u64) {
      self.progbar.set_message("Connected");
      self.progbar.set_length(total_length);
      self.progbar.set_position(total_length - content_length);
   }

   fn complete(&mut self) {
      self.progbar.set_style(self.style.finished.clone());
      self.progbar.finish_with_message(&format!(
         "Completed: {}.part{}",
         self.output.display(),
         self.part
      ));
   }

   fn update(&mut self, new_progress: usize) {
      self.progbar.inc(new_progress as u64);
   }

   fn fail(&mut self) {
      self.progbar.set_style(self.style.failed.clone());
      self.progbar.finish_with_message(&format!(
         "Failed   : {}.part{}",
         self.output.display(),
         self.part
      ));
   }
}
