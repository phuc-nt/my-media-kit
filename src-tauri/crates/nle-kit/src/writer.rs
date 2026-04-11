//! Small wrapper over quick-xml to keep exporter code readable. Exposes
//! helpers for opening/closing tags, emitting self-closing elements with
//! attributes, and writing CDATA-safe text nodes.

use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use std::io::Cursor;

pub struct XmlBuilder {
    writer: Writer<Cursor<Vec<u8>>>,
}

impl XmlBuilder {
    pub fn new(indent: bool) -> Self {
        let inner = Cursor::new(Vec::new());
        let writer = if indent {
            Writer::new_with_indent(inner, b' ', 4)
        } else {
            Writer::new(inner)
        };
        Self { writer }
    }

    pub fn declaration(&mut self) {
        self.writer
            .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
            .expect("write decl");
    }

    pub fn doctype(&mut self, dt: &str) {
        self.writer
            .write_event(Event::DocType(BytesText::from_escaped(dt)))
            .expect("write doctype");
    }

    /// Emit an opening tag with attributes. Returns so the caller can stash
    /// the tag name for closing later.
    pub fn open(&mut self, name: &str, attrs: &[(&str, String)]) {
        let mut start = BytesStart::new(name);
        for (k, v) in attrs {
            start.push_attribute((*k, v.as_str()));
        }
        self.writer
            .write_event(Event::Start(start))
            .expect("write open");
    }

    pub fn close(&mut self, name: &str) {
        self.writer
            .write_event(Event::End(BytesEnd::new(name)))
            .expect("write close");
    }

    pub fn empty(&mut self, name: &str, attrs: &[(&str, String)]) {
        let mut start = BytesStart::new(name);
        for (k, v) in attrs {
            start.push_attribute((*k, v.as_str()));
        }
        self.writer
            .write_event(Event::Empty(start))
            .expect("write empty");
    }

    pub fn text(&mut self, value: &str) {
        self.writer
            .write_event(Event::Text(BytesText::new(value)))
            .expect("write text");
    }

    /// Convenience for `<tag>text</tag>` one-liners.
    pub fn text_element(&mut self, name: &str, value: &str) {
        self.open(name, &[]);
        self.text(value);
        self.close(name);
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.writer.into_inner().into_inner()
    }
}
