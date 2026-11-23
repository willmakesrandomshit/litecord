use crate::model::Message;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VirtualRange {
    pub start: usize,
    pub end: usize,
    pub top_spacer: f32,
    pub bottom_spacer: f32,
    pub total_height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollAnchor {
    Bottom,
    Item { index: usize, offset_in_item: f32 },
}

/// Estimates the rendered pixel height of a message row given layout parameters.
pub fn estimate_message_height(
    msg: &Message,
    content_width: f32,
    font_size: f32,
    compact_mode: bool,
) -> f32 {
    let usable_width = (content_width - 76.0).max(160.0);
    let avg_char_width = (font_size * 0.54).max(5.5);
    let chars_per_line = (usable_width / avg_char_width).floor().max(16.0) as usize;

    let mut lines: usize = 0;
    for raw_line in msg.content.split('\n') {
        let len = raw_line.chars().count();
        lines += len.div_ceil(chars_per_line).max(1);
    }
    lines = lines.max(1);

    let line_height = font_size * 1.38;
    let header_height = 20.0;
    let reactions_height = if msg.reactions.is_empty() { 22.0 } else { 28.0 };
    let vertical_pad = if compact_mode { 4.0 } else { 8.0 };

    (header_height + (lines as f32 * line_height) + reactions_height + vertical_pad).ceil()
}

/// Prefix-sum backed virtualized timeline supporting O(log N) visible-range queries,
/// O(1) incremental appends, and stable scroll anchoring.
#[derive(Debug, Clone)]
pub struct VirtualTimeline {
    heights: Vec<f32>,
    prefix: Vec<f32>,
    spacing: f32,
}

impl VirtualTimeline {
    pub fn new(spacing: f32) -> Self {
        Self {
            heights: Vec::new(),
            prefix: vec![0.0],
            spacing,
        }
    }

    pub fn len(&self) -> usize {
        self.heights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }

    pub fn total_height(&self) -> f32 {
        *self.prefix.last().unwrap_or(&0.0)
    }

    pub fn rebuild<'a, I>(&mut self, iter: I, spacing: f32)
    where
        I: Iterator<Item = &'a Message>,
    {
        self.spacing = spacing;
        self.heights.clear();
        self.prefix.clear();
        self.prefix.push(0.0);

        // Default layout estimate; caller can pass exact width via rebuild_with_layout.
        for msg in iter {
            let h = estimate_message_height(msg, 720.0, 14.0, false);
            self.push_height(h);
        }
    }

    pub fn rebuild_with_layout<'a, I>(
        &mut self,
        iter: I,
        content_width: f32,
        font_size: f32,
        compact_mode: bool,
        spacing: f32,
    ) where
        I: Iterator<Item = &'a Message>,
    {
        self.spacing = spacing;
        self.heights.clear();
        self.prefix.clear();
        self.prefix.push(0.0);

        for msg in iter {
            let h = estimate_message_height(msg, content_width, font_size, compact_mode);
            self.push_height(h);
        }
    }

    /// O(1) incremental append.
    pub fn push_height(&mut self, height: f32) {
        let prev_total = *self.prefix.last().unwrap_or(&0.0);
        let gap = if self.heights.is_empty() {
            0.0
        } else {
            self.spacing
        };
        self.heights.push(height);
        self.prefix.push(prev_total + gap + height);
    }

    /// Updates a single item's height (e.g. after reaction toggle) and shifts subsequent prefix sums.
    pub fn update_height(&mut self, index: usize, new_height: f32) {
        if index >= self.heights.len() {
            return;
        }
        let delta = new_height - self.heights[index];
        if delta.abs() < f32::EPSILON {
            return;
        }
        self.heights[index] = new_height;
        for p in &mut self.prefix[index + 1..] {
            *p += delta;
        }
    }

    /// Computes the visible slice `[start, end)` with `overscan` items above and below in O(log N).
    pub fn visible_range(
        &self,
        scroll_y: f32,
        viewport_height: f32,
        overscan: usize,
    ) -> VirtualRange {
        let n = self.heights.len();
        if n == 0 {
            return VirtualRange {
                start: 0,
                end: 0,
                top_spacer: 0.0,
                bottom_spacer: 0.0,
                total_height: 0.0,
            };
        }

        let total_height = self.total_height();
        let top = scroll_y.max(0.0);
        let bottom = (top + viewport_height.max(1.0)).min(total_height);

        // First item whose bottom edge (prefix[i + 1]) > top.
        let first_visible = self.prefix[1..].partition_point(|&p| p <= top);
        let first_visible = first_visible.min(n.saturating_sub(1));

        // First item whose top edge (prefix[i]) >= bottom.
        let past_visible = self.prefix[..n].partition_point(|&p| p < bottom);

        let start = first_visible.saturating_sub(overscan);
        let end = (past_visible + overscan).min(n).max(start + 1);

        let top_spacer = self.prefix[start];
        let bottom_spacer = (total_height - self.prefix[end]).max(0.0);

        VirtualRange {
            start,
            end,
            top_spacer,
            bottom_spacer,
            total_height,
        }
    }

    /// Captures a stable scroll anchor for the current scroll position.
    pub fn capture_anchor(&self, scroll_y: f32, viewport_height: f32) -> ScrollAnchor {
        let total = self.total_height();
        if self.heights.is_empty() || scroll_y + viewport_height >= total - 24.0 {
            return ScrollAnchor::Bottom;
        }
        let idx = self.prefix[1..]
            .partition_point(|&p| p <= scroll_y.max(0.0))
            .min(self.heights.len().saturating_sub(1));
        let offset_in_item = (scroll_y - self.prefix[idx]).max(0.0);
        ScrollAnchor::Item {
            index: idx,
            offset_in_item,
        }
    }

    /// Resolves a previously captured anchor to a scroll_y offset.
    pub fn resolve_anchor(&self, anchor: ScrollAnchor, viewport_height: f32) -> f32 {
        match anchor {
            ScrollAnchor::Bottom => (self.total_height() - viewport_height).max(0.0),
            ScrollAnchor::Item {
                index,
                offset_in_item,
            } => {
                if self.heights.is_empty() {
                    0.0
                } else {
                    let clamped = index.min(self.heights.len() - 1);
                    (self.prefix[clamped] + offset_in_item)
                        .min((self.total_height() - viewport_height).max(0.0))
                        .max(0.0)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ChannelId, MessageId, UserId};
    use chrono::Utc;

    fn make_msg(id: u64, content: &str) -> Message {
        Message {
            id: MessageId(id),
            channel_id: ChannelId(1),
            author_id: UserId(1),
            content: content.to_string(),
            timestamp: Utc::now(),
            edited: false,
            pinned: false,
            reactions: Vec::new(),
            reply_to: None,
        }
    }

    #[test]
    fn virtual_range_bounds_rendered_items_for_100k_messages() {
        let mut vt = VirtualTimeline::new(8.0);
        for i in 0..100_000 {
            vt.push_height(64.0 + (i % 3) as f32 * 18.0);
        }
        assert_eq!(vt.len(), 100_000);

        let range = vt.visible_range(2_500_000.0, 800.0, 6);
        let rendered_count = range.end - range.start;
        assert!(rendered_count <= 26, "rendered_count={rendered_count}");
        assert!(range.top_spacer > 0.0);
        assert!(range.bottom_spacer > 0.0);
        assert!(
            (range.top_spacer + range.bottom_spacer) < range.total_height,
            "spacers must leave room for visible items"
        );
    }

    #[test]
    fn scroll_anchor_tracks_bottom_and_item_offsets() {
        let mut vt = VirtualTimeline::new(10.0);
        for _ in 0..50 {
            vt.push_height(70.0);
        }
        let bottom_anchor = vt.capture_anchor(vt.total_height() - 600.0, 600.0);
        assert_eq!(bottom_anchor, ScrollAnchor::Bottom);

        vt.push_height(90.0);
        let new_y = vt.resolve_anchor(bottom_anchor, 600.0);
        assert!((new_y - (vt.total_height() - 600.0)).abs() < 0.01);

        let mid_anchor = vt.capture_anchor(800.0, 600.0);
        assert!(matches!(mid_anchor, ScrollAnchor::Item { .. }));
        let resolved_mid = vt.resolve_anchor(mid_anchor, 600.0);
        assert!((resolved_mid - 800.0).abs() < 0.01);
    }

    #[test]
    fn variable_height_estimation_scales_with_lines() {
        let short = make_msg(1, "hi");
        let long = make_msg(2, &"word ".repeat(80));
        let h_short = estimate_message_height(&short, 700.0, 14.0, false);
        let h_long = estimate_message_height(&long, 700.0, 14.0, false);
        assert!(h_long > h_short * 2.0);
    }
}
