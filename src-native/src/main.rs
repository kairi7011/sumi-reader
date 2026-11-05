use sumi_native::pdf;
mod reader;
use sumi_native::search;
mod ui;

use std::{cell::RefCell, collections::VecDeque, path::PathBuf};
use objc2_foundation::NSPoint;

pub(crate) enum Command {
    Action(String, usize),
    Outside(NSPoint),
    Files(Vec<PathBuf>),
    BookAction(usize, String),
}
thread_local! {
    pub(crate) static COMMANDS: RefCell<VecDeque<Command>> = const { RefCell::new(VecDeque::new()) };
}
pub fn enqueue(command: &str, value: usize) {
    COMMANDS.with(|q| q.borrow_mut().push_back(Command::Action(command.into(), value)));
}
fn main() { ui::run(); }
