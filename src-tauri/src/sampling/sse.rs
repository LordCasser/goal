//! Incremental server-sent-events framer shared by all three adapters.
//!
//! The protocols all ship `data: {...}` lines; Anthropic and the OpenAI
//! Responses API additionally set an `event:` line per block (design D2).
//! This module only frames blocks — adapters interpret the payloads.

/// One decoded SSE block: the optional event name and the concatenated data
/// payload (`data:` lines joined with `\n`, per the SSE grammar).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

/// Byte-level SSE framer. Feed raw body chunks in arrival order; it buffers
/// partial lines and emits a block each time a blank line completes one.
#[derive(Debug, Default)]
pub(crate) struct SseParser {
    /// Bytes of the current, not yet `\n`-terminated line.
    line: Vec<u8>,
    /// `event:` value collected for the block under construction.
    event: Option<String>,
    /// `data:` values collected for the block under construction.
    data: Vec<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one body chunk and returns every block it completed.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        let mut events = Vec::new();
        for &byte in chunk {
            if byte == b'\n' {
                self.line_completed(&mut events);
            } else {
                self.line.push(byte);
            }
        }
        events
    }

    /// Flushes at body end: a final line without trailing newline is still
    /// processed, and a block left open by a server that omitted the last
    /// blank line is emitted rather than dropped.
    pub fn finish(&mut self) -> Vec<SseEvent> {
        let mut events = Vec::new();
        if !self.line.is_empty() {
            self.line_completed(&mut events);
        }
        if let Some(event) = self.close_block() {
            events.push(event);
        }
        events
    }

    /// Handles one `\n`-terminated line: a blank line closes the current
    /// block, anything else accumulates into it.
    fn line_completed(&mut self, events: &mut Vec<SseEvent>) {
        // Tolerate CRLF: the `\r` belongs to the terminator, not the value.
        if self.line.last() == Some(&b'\r') {
            self.line.pop();
        }
        let line = String::from_utf8_lossy(&self.line).into_owned();
        self.line.clear();
        if line.is_empty() {
            if let Some(event) = self.close_block() {
                events.push(event);
            }
            return;
        }
        // `field: value` with one optional space after the colon; a leading
        // colon marks a comment, and fields we do not use (`id`, `retry`)
        // are ignored along with lines without a colon.
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line.as_str(), ""),
        };
        match field {
            "event" => self.event = Some(value.to_string()),
            "data" => self.data.push(value.to_string()),
            _ => {}
        }
    }

    /// Ends the block under construction, returning it when it carries data.
    fn close_block(&mut self) -> Option<SseEvent> {
        if self.data.is_empty() {
            self.event = None;
            return None;
        }
        Some(SseEvent {
            event: self.event.take(),
            data: std::mem::take(&mut self.data).join("\n"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(payload: &str) -> Vec<SseEvent> {
        let mut parser = SseParser::new();
        let mut events = parser.feed(payload.as_bytes());
        events.extend(parser.finish());
        events
    }

    #[test]
    fn frames_event_and_data_lines() {
        let events = all("event: content_block_delta\ndata: {\"a\":1}\n\n");
        assert_eq!(
            events,
            vec![SseEvent {
                event: Some("content_block_delta".into()),
                data: "{\"a\":1}".into()
            }]
        );
    }

    #[test]
    fn splits_lines_across_chunk_boundaries() {
        // Byte-by-byte delivery must not corrupt lines or UTF-8 payloads.
        let payload = "data: {\"text\":\"héllo\"}\n\ndata: [DONE]\n\n";
        let mut parser = SseParser::new();
        let mut events = Vec::new();
        for byte in payload.as_bytes() {
            events.extend(parser.feed(&[*byte]));
        }
        events.extend(parser.finish());
        assert_eq!(
            events,
            vec![
                SseEvent {
                    event: None,
                    data: "{\"text\":\"héllo\"}".into()
                },
                SseEvent {
                    event: None,
                    data: "[DONE]".into()
                },
            ]
        );
    }

    #[test]
    fn handles_crlf_comments_and_ignored_fields() {
        let events = all(": keep-alive\r\nid: 7\r\nretry: 100\r\nevent:ping\r\ndata: x\r\n\r\n");
        assert_eq!(
            events,
            vec![SseEvent {
                event: Some("ping".into()),
                data: "x".into()
            }]
        );
    }

    #[test]
    fn joins_multiple_data_lines_with_newline() {
        let events = all("data: first\ndata: second\n\n");
        assert_eq!(
            events,
            vec![SseEvent {
                event: None,
                data: "first\nsecond".into()
            }]
        );
    }

    #[test]
    fn finish_flushes_block_without_trailing_blank_line() {
        let events = all("data: tail");
        assert_eq!(
            events,
            vec![SseEvent {
                event: None,
                data: "tail".into()
            }]
        );
    }
}
