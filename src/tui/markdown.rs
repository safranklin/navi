//! Markdown → ratatui `Text` renderer.
//!
//! Thin wrapper around `pulldown_cmark` that converts markdown events into
//! styled `Line`/`Span` values. Headings, bold, italic, inline code, fenced
//! code blocks (with syntect highlighting), lists, blockquotes, and links.

use std::sync::LazyLock;

use pulldown_cmark::{CodeBlockKind, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// Parse markdown content into styled `Text` using Navi's color scheme.
///
/// Returns owned text (`'static`) so callers aren't constrained by input lifetime.
pub fn render(content: &str, base_fg: Color) -> Text<'static> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);

    let events: Vec<Event<'_>> = Parser::new_ext(content, opts).collect();
    let mut w = Writer::new(base_fg);
    for event in events {
        w.handle(event);
    }
    w.text
}

// ── Writer ──────────────────────────────────────────────────────────────────

struct Writer {
    text: Text<'static>,
    base_fg: Color,
    /// Inline style stack (bold, italic, heading text, etc.). Styles compose
    /// via `patch` so nested bold+italic works.
    styles: Vec<Style>,
    /// Per-line prefix spans (blockquote `│`).
    line_prefixes: Vec<Span<'static>>,
    /// List nesting: None = unordered, Some(n) = ordered at index n.
    list_indices: Vec<Option<u64>>,
    /// Active syntax highlighter for fenced code blocks.
    highlighter: Option<HighlightLines<'static>>,
    /// True when inside a fenced code block without syntax highlighting.
    in_plain_code: bool,
    /// Stored link URL, appended after the link text closes.
    link_url: Option<String>,
    /// Whether the next block element should be preceded by a blank line.
    needs_newline: bool,
}

impl Writer {
    fn new(base_fg: Color) -> Self {
        Self {
            text: Text::default(),
            base_fg,
            styles: vec![],
            line_prefixes: vec![],
            list_indices: vec![],
            highlighter: None,
            in_plain_code: false,
            link_url: None,
            needs_newline: false,
        }
    }

    // ── Style helpers ───────────────────────────────────────────────────

    /// Current effective style: top of stack, or base foreground color.
    fn style(&self) -> Style {
        self.styles
            .last()
            .copied()
            .unwrap_or_else(|| Style::default().fg(self.base_fg))
    }

    /// Push a style that composes with the current one (inherits parent modifiers).
    fn push_style(&mut self, overlay: Style) {
        self.styles.push(self.style().patch(overlay));
    }

    fn pop_style(&mut self) {
        self.styles.pop();
    }

    // ── Line/span helpers ───────────────────────────────────────────────

    fn push_line(&mut self, line: Line<'static>) {
        let mut out = line;
        for pfx in self.line_prefixes.iter().rev().cloned() {
            out.spans.insert(0, pfx);
        }
        self.text.lines.push(out);
    }

    fn push_span(&mut self, span: Span<'static>) {
        if let Some(line) = self.text.lines.last_mut() {
            line.push_span(span);
        } else {
            self.push_line(Line::from(vec![span]));
        }
    }

    fn blank_line_if_needed(&mut self) {
        if self.needs_newline {
            self.push_line(Line::default());
            self.needs_newline = false;
        }
    }

    // ── Event dispatch ──────────────────────────────────────────────────

    fn handle(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.open(tag),
            Event::End(tag) => self.close(tag),
            Event::Text(t) => self.text(t),
            Event::Code(c) => self.inline_code(c),
            Event::SoftBreak => self.push_span(Span::raw(" ")),
            Event::HardBreak => self.push_line(Line::default()),
            Event::Rule => {
                self.blank_line_if_needed();
                self.push_line(Line::from(Span::styled(
                    "─".repeat(40),
                    Style::default().fg(Color::DarkGray),
                )));
                self.needs_newline = true;
            }
            Event::TaskListMarker(checked) => {
                let marker = if checked { "[x] " } else { "[ ] " };
                self.push_span(Span::raw(marker));
            }
            _ => {} // HTML, footnotes, math — skip
        }
    }

    fn open(&mut self, tag: Tag<'_>) {
        match tag {
            // ── Block elements ──────────────────────────────────────────
            Tag::Paragraph => {
                self.blank_line_if_needed();
                self.push_line(Line::default());
            }
            Tag::Heading { level, .. } => {
                self.blank_line_if_needed();
                let hs = heading_style(self.base_fg, level);
                let depth = heading_depth(level) as usize;
                self.push_line(Line::from(Span::styled(
                    format!("{} ", "#".repeat(depth)),
                    hs,
                )));
                // Push heading style so text() inherits it — this is the
                // bug fix over tui-markdown, which only styled the `##` prefix.
                self.push_style(hs);
            }
            Tag::BlockQuote(_) => {
                self.blank_line_if_needed();
                self.line_prefixes.push(Span::styled(
                    "│ ",
                    Style::default().fg(Color::DarkGray),
                ));
                self.push_style(
                    Style::default()
                        .fg(self.base_fg)
                        .add_modifier(Modifier::DIM | Modifier::ITALIC),
                );
            }
            Tag::CodeBlock(kind) => {
                if !self.text.lines.is_empty() {
                    self.push_line(Line::default());
                }
                let lang = match &kind {
                    CodeBlockKind::Fenced(l) => l.as_ref(),
                    CodeBlockKind::Indented => "",
                };

                // Top border: ╭── lang  or just ╭──
                let bs = Style::default().fg(Color::DarkGray);
                let top = if lang.is_empty() {
                    Line::from(Span::styled("╭──", bs))
                } else {
                    Line::from(vec![
                        Span::styled("╭── ", bs),
                        Span::styled(
                            lang.to_owned(),
                            bs.add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(" ──", bs),
                    ])
                };
                self.push_line(top);

                // Left border prefix for code content
                self.line_prefixes
                    .push(Span::styled("│ ", bs));

                // Syntax highlighting setup
                if !lang.is_empty()
                    && let Some(syn) = SYNTAX_SET.find_syntax_by_token(lang)
                {
                    let theme = &THEME_SET.themes["base16-ocean.dark"];
                    self.highlighter = Some(HighlightLines::new(syn, theme));
                }
                if self.highlighter.is_none() {
                    self.in_plain_code = true;
                }
            }
            Tag::List(start) => {
                if self.list_indices.is_empty() {
                    self.blank_line_if_needed();
                }
                self.list_indices.push(start);
            }
            Tag::Item => {
                self.push_line(Line::default());
                let depth = self.list_indices.len().saturating_sub(1);
                let indent = "  ".repeat(depth);
                if let Some(idx) = self.list_indices.last_mut() {
                    let marker = match idx {
                        None => format!("{indent}- "),
                        Some(n) => {
                            let s = format!("{indent}{}. ", n);
                            *n += 1;
                            s
                        }
                    };
                    self.push_span(Span::styled(
                        marker,
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }

            // ── Inline elements ─────────────────────────────────────────
            Tag::Emphasis => self.push_style(Style::default().add_modifier(Modifier::ITALIC)),
            Tag::Strong => self.push_style(Style::default().add_modifier(Modifier::BOLD)),
            Tag::Strikethrough => {
                self.push_style(Style::default().add_modifier(Modifier::CROSSED_OUT))
            }
            Tag::Link { dest_url, .. } => {
                self.link_url = Some(dest_url.to_string());
                self.push_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::UNDERLINED),
                );
            }
            _ => {} // Tables, images, definitions — skip
        }
    }

    fn close(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.needs_newline = true,
            TagEnd::Heading(_) => {
                self.pop_style();
                self.needs_newline = true;
            }
            TagEnd::BlockQuote(_) => {
                self.line_prefixes.pop();
                self.pop_style();
                self.needs_newline = true;
            }
            TagEnd::CodeBlock => {
                self.highlighter = None;
                self.in_plain_code = false;
                self.line_prefixes.pop(); // remove │ prefix before bottom border
                let bs = Style::default().fg(Color::DarkGray);
                self.push_line(Line::from(Span::styled("╰──", bs)));
                self.needs_newline = true;
            }
            TagEnd::List(_) => {
                self.list_indices.pop();
                self.needs_newline = true;
            }
            TagEnd::Item => {}
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => self.pop_style(),
            TagEnd::Link => {
                self.pop_style();
                if let Some(url) = self.link_url.take() {
                    self.push_span(Span::raw(" ("));
                    self.push_span(Span::styled(
                        url,
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::UNDERLINED),
                    ));
                    self.push_span(Span::raw(")"));
                }
            }
            _ => {}
        }
    }

    // ── Content handlers ────────────────────────────────────────────────

    fn text(&mut self, cow: CowStr<'_>) {
        // Expand tabs → 4 spaces (ratatui renders \t as zero-width)
        let raw = cow.to_string();
        let text = if raw.contains('\t') {
            raw.replace('\t', "    ")
        } else {
            raw
        };

        // Syntax-highlighted code block — take highlighter out to avoid
        // double-mutable-borrow (highlight_line borrows it, push_line borrows self)
        if self.highlighter.is_some() {
            let mut hl = self.highlighter.take().unwrap();
            for line in LinesWithEndings::from(text.as_str()) {
                if let Ok(ranges) = hl.highlight_line(line, &SYNTAX_SET) {
                    let spans: Vec<Span<'static>> = ranges
                        .into_iter()
                        .filter_map(|(hl_style, frag)| {
                            let content = frag.trim_end_matches('\n').replace('\t', "    ");
                            if content.is_empty() {
                                return None;
                            }
                            let fg = Color::Rgb(
                                hl_style.foreground.r,
                                hl_style.foreground.g,
                                hl_style.foreground.b,
                            );
                            Some(Span::styled(content, Style::default().fg(fg)))
                        })
                        .collect();
                    if !spans.is_empty() {
                        self.push_line(Line::from(spans));
                    }
                }
            }
            self.highlighter = Some(hl);
            return;
        }

        // Plain code block (no highlighting available)
        if self.in_plain_code {
            let code_style = Style::default().fg(Color::White);
            for line in text.lines() {
                self.push_line(Line::from(Span::styled(line.to_owned(), code_style)));
            }
            return;
        }

        // Normal text — inherits current style (heading, bold, etc.)
        let style = self.style();
        self.push_span(Span::styled(text, style));
    }

    fn inline_code(&mut self, cow: CowStr<'_>) {
        let style = Style::default().fg(Color::White).bg(Color::DarkGray);
        self.push_span(Span::styled(cow.to_string(), style));
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn heading_style(base_fg: Color, level: HeadingLevel) -> Style {
    match level {
        HeadingLevel::H1 => Style::default()
            .fg(base_fg)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        HeadingLevel::H2 => Style::default()
            .fg(base_fg)
            .add_modifier(Modifier::BOLD),
        _ => Style::default()
            .fg(base_fg)
            .add_modifier(Modifier::BOLD | Modifier::ITALIC),
    }
}

fn heading_depth(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_text_inherits_heading_style() {
        let text = render("## Hello", Color::Blue);
        // Line 0 should contain "## " and "Hello", both with bold + blue
        let line = &text.lines[0];
        assert!(line.spans.len() >= 2, "expected >= 2 spans, got {:?}", line);
        let prefix_style = line.spans[0].style;
        let text_style = line.spans[1].style;
        // Both should have BOLD and blue foreground
        assert!(prefix_style.add_modifier.contains(Modifier::BOLD));
        assert!(text_style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(text_style.fg, Some(Color::Blue));
    }

    #[test]
    fn bold_text_is_bold() {
        let text = render("Some **bold** text", Color::Blue);
        let line = &text.lines[0];
        // Find the "bold" span
        let bold_span = line.spans.iter().find(|s| s.content == "bold").unwrap();
        assert!(bold_span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn inline_code_styled() {
        let text = render("Use `foo()` here", Color::Blue);
        let line = &text.lines[0];
        let code_span = line.spans.iter().find(|s| s.content == "foo()").unwrap();
        assert_eq!(code_span.style.fg, Some(Color::White));
        assert_eq!(code_span.style.bg, Some(Color::DarkGray));
    }

    #[test]
    fn code_block_has_border_structure() {
        let text = render("```\nline1\nline2\n```", Color::Blue);
        let all_content: Vec<String> = text
            .lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect();
        // Top border
        assert!(all_content[0].starts_with('╭'), "expected top border, got {:?}", all_content[0]);
        // Content lines with left border
        assert!(all_content[1].starts_with("│ "), "expected │ prefix, got {:?}", all_content[1]);
        assert!(all_content[1].contains("line1"));
        assert!(all_content[2].starts_with("│ "), "expected │ prefix, got {:?}", all_content[2]);
        assert!(all_content[2].contains("line2"));
        // Bottom border
        let last = all_content.last().unwrap();
        assert!(last.starts_with('╰'), "expected bottom border, got {:?}", last);
    }

    #[test]
    fn plain_text_uses_base_color() {
        let text = render("hello", Color::Green);
        let line = &text.lines[0];
        let span = &line.spans[0];
        assert_eq!(span.style.fg, Some(Color::Green));
    }

    #[test]
    fn tabs_expanded_to_spaces() {
        let text = render("```\n\tindented\n```", Color::Blue);
        let has_spaces = text.lines.iter().any(|l| {
            l.spans.iter().any(|s| s.content.starts_with("    "))
        });
        assert!(has_spaces, "tabs should be expanded to 4 spaces");
        let has_tabs = text.lines.iter().any(|l| {
            l.spans.iter().any(|s| s.content.contains('\t'))
        });
        assert!(!has_tabs, "no raw tabs should remain");
    }
}
