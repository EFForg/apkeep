use std::rc::Rc;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::consts;

pub fn progress_wrapper(mp: Rc<MultiProgress>) -> Box<dyn Fn(String, u64) -> Box<dyn Fn(u64) -> ()>> {
    Box::new(move |filename, length| {
        let mp1 = Rc::clone(&mp);
        let mp2 = Rc::clone(&mp);
        let pb = ProgressBar::new(length).with_message(filename);
        pb.set_style(ProgressStyle::with_template(
                consts::PROGRESS_STYLE).unwrap());
        let pb = mp1.add(pb);
        Box::new(move |downloaded| {
            if !pb.is_finished() {
                pb.set_position(downloaded);
                if length == downloaded {
                    pb.finish();
                    mp2.remove(&pb);
                }
            }
        })
    })
}
