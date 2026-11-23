#![forbid(unsafe_code)]

pub mod markdown;
pub mod model;
pub mod state;
pub mod virtual_list;

pub use markdown::{parse_message_segments, MessageSegment};
pub use model::*;
pub use state::{ZeroState, MAX_CACHED_MESSAGES_PER_CHANNEL};
pub use virtual_list::{estimate_message_height, ScrollAnchor, VirtualRange, VirtualTimeline};
