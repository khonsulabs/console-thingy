use std::collections::VecDeque;

use crate::wrap::Wrapped;

#[derive(Default)]
pub struct Scrollback {
    pub events: VecDeque<Wrapped>,
    pub scroll: usize,
    pub maximum_scroll: usize,
    pub columns: usize,
}
