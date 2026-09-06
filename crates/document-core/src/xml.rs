//! Small helpers shared by both XML readers and writers.

/// Escape text for an XML attribute value.
pub fn esc_attr(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            '\t' => o.push_str("&#9;"),
            '\n' => o.push_str("&#10;"),
            '\r' => o.push_str("&#13;"),
            c if is_xml_char(c) => o.push(c),
            _ => {}
        }
    }
    o
}

/// Escape text for XML character data.
pub fn esc_text(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            c if is_xml_char(c) => o.push(c),
            _ => {}
        }
    }
    o
}

/// XML 1.0 legal character range. Anything outside it is dropped rather than written,
/// because emitting it would produce a file no parser can read back.
pub fn is_xml_char(c: char) -> bool {
    matches!(c, '\u{9}' | '\u{A}' | '\u{D}')
        || ('\u{20}'..='\u{D7FF}').contains(&c)
        || ('\u{E000}'..='\u{FFFD}').contains(&c)
        || ('\u{10000}'..='\u{10FFFF}').contains(&c)
}

/// Strip characters XML cannot represent, keeping everything else including all Turkish
/// letters. Used on text pulled out of a source document before it is written out again.
pub fn sanitize_text(s: &str) -> String {
    if s.chars().all(is_xml_char) {
        return s.to_string();
    }
    s.chars().filter(|c| is_xml_char(*c)).collect()
}

/// A CDATA section cannot contain the literal `]]>`. Split it across two sections instead,
/// which is the standard, lossless encoding.
pub fn cdata_safe(s: &str) -> String {
    if !s.contains("]]>") {
        return s.to_string();
    }
    s.replace("]]>", "]]]]><![CDATA[>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turkish_characters_survive_escaping() {
        let t = "Çağdaş Türkiye Cumhuriyeti — İİK, HMK, ıİğĞüÜşŞöÖçÇ";
        assert_eq!(esc_text(t), t);
        assert_eq!(esc_attr(t), t);
        assert_eq!(sanitize_text(t), t);
    }

    #[test]
    fn markup_is_escaped() {
        assert_eq!(esc_text("a<b>&c"), "a&lt;b&gt;&amp;c");
        assert_eq!(esc_attr("a\"b'c"), "a&quot;b&apos;c");
    }

    #[test]
    fn illegal_control_chars_are_dropped_not_emitted() {
        assert_eq!(sanitize_text("a\u{0}b\u{1}c"), "abc");
        assert_eq!(sanitize_text("a\tb\nc"), "a\tb\nc");
    }

    #[test]
    fn cdata_terminator_is_split_losslessly() {
        let s = cdata_safe("before]]>after");
        assert!(!s.contains("]]>after"));
        // Reassembling the CDATA sections yields the original text.
        assert_eq!(s.replace("]]]]><![CDATA[>", "]]>"), "before]]>after");
    }

    #[test]
    fn astral_characters_are_legal_xml() {
        assert!(is_xml_char('\u{1F600}'));
        assert_eq!(sanitize_text("a\u{1F600}b"), "a\u{1F600}b");
    }
}
