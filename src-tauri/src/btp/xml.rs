//! VISUALXML-Codec: Encoder für Requests, Decoder für Responses.
//!
//! VISUALXML besteht aus `GROUP`-Containern und skalaren `ITEM`-Elementen,
//! jeweils mit `ID`-Attribut. Siehe `docs/btp_protocol.md`.

use std::fmt::Write as _;

use quick_xml::escape::escape;
use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

/// Strukturiertes Datum aus einem `<DATETIME>`-Element.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DateTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millis: u32,
}

/// Wert eines `ITEM`, abhängig vom `TYPE`-Attribut.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
    DateTime(DateTime),
}

/// Knoten im VISUALXML-Baum: Container (`GROUP`) oder Skalar (`ITEM`).
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Group { id: String, children: Vec<Node> },
    Item { id: String, value: Value },
}

impl Node {
    /// Konstruiert einen `GROUP`-Knoten.
    pub fn group(id: impl Into<String>, children: Vec<Node>) -> Node {
        Node::Group {
            id: id.into(),
            children,
        }
    }

    /// Konstruiert ein `ITEM` mit String-Wert.
    pub fn string(id: impl Into<String>, value: impl Into<String>) -> Node {
        Node::Item {
            id: id.into(),
            value: Value::String(value.into()),
        }
    }

    /// Konstruiert ein `ITEM` mit Integer-Wert.
    pub fn integer(id: impl Into<String>, value: i64) -> Node {
        Node::Item {
            id: id.into(),
            value: Value::Integer(value),
        }
    }

    /// ID dieses Knotens.
    pub fn id(&self) -> &str {
        match self {
            Node::Group { id, .. } | Node::Item { id, .. } => id,
        }
    }

    /// Kinder eines `GROUP`-Knotens; leer bei einem `ITEM`.
    pub fn children(&self) -> &[Node] {
        match self {
            Node::Group { children, .. } => children,
            Node::Item { .. } => &[],
        }
    }

    /// Wert eines `ITEM`-Knotens; `None` bei einer `GROUP`.
    pub fn value(&self) -> Option<&Value> {
        match self {
            Node::Item { value, .. } => Some(value),
            Node::Group { .. } => None,
        }
    }
}

impl Value {
    /// Wert als String, falls es ein String-Wert ist.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Wert als Integer, falls es ein Integer-Wert ist.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Wert als Boolean, falls es ein Bool-Wert ist.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// Findet den ersten Knoten mit der gegebenen ID in einer Knotenliste.
pub fn find<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Node> {
    nodes.iter().find(|n| n.id() == id)
}

#[derive(Debug, thiserror::Error)]
pub enum XmlError {
    #[error("XML-Fehler: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("fehlendes Attribut '{0}'")]
    MissingAttribute(String),
    #[error("unbekanntes Element '{0}'")]
    UnknownElement(String),
    #[error("unbekannter ITEM-TYPE '{0}'")]
    UnknownType(String),
    #[error("unbekannte XML-Entity '&{0};'")]
    UnknownEntity(String),
    #[error("ungültiger Wert für {ty}: '{raw}'")]
    InvalidValue { ty: &'static str, raw: String },
    #[error("unerwartetes Ende des XML-Dokuments")]
    UnexpectedEof,
}

// ---------------------------------------------------------------- Encoder ---

/// Kodiert die Top-Level-Knoten als vollständiges VISUALXML-Dokument.
pub fn encode(nodes: &[Node]) -> String {
    let mut out =
        String::from(r#"<?xml version="1.0" encoding="UTF-8"?><VISUALXML VERSION="1.0">"#);
    for node in nodes {
        encode_node(node, &mut out);
    }
    out.push_str("</VISUALXML>");
    out
}

fn encode_node(node: &Node, out: &mut String) {
    match node {
        Node::Group { id, children } => {
            out.push_str("<GROUP ID=\"");
            out.push_str(&escape(id.as_str()));
            out.push_str("\">");
            for child in children {
                encode_node(child, out);
            }
            out.push_str("</GROUP>");
        }
        Node::Item { id, value } => {
            out.push_str("<ITEM ID=\"");
            out.push_str(&escape(id.as_str()));
            out.push_str("\" TYPE=\"");
            out.push_str(value_type(value));
            out.push_str("\">");
            encode_value_body(value, out);
            out.push_str("</ITEM>");
        }
    }
}

fn value_type(v: &Value) -> &'static str {
    match v {
        Value::String(_) => "String",
        Value::Integer(_) => "Integer",
        Value::Float(_) => "Float",
        Value::Bool(_) => "Bool",
        Value::DateTime(_) => "DateTime",
    }
}

fn encode_value_body(v: &Value, out: &mut String) {
    match v {
        Value::String(s) => out.push_str(&escape(s.as_str())),
        Value::Integer(i) => {
            let _ = write!(out, "{i}");
        }
        Value::Float(f) => {
            let _ = write!(out, "{f}");
        }
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::DateTime(dt) => {
            let _ = write!(
                out,
                "<DATETIME Y=\"{}\" MM=\"{}\" D=\"{}\" H=\"{}\" M=\"{}\" S=\"{}\" MS=\"{}\"/>",
                dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second, dt.millis
            );
        }
    }
}

// ---------------------------------------------------------------- Decoder ---

/// Dekodiert ein VISUALXML-Dokument zu seinen Top-Level-Knoten.
pub fn decode(xml: &str) -> Result<Vec<Node>, XmlError> {
    let mut reader = Reader::from_str(xml);
    // Stack der Kinder-Sammler offener GROUPs; das unterste Element sammelt
    // die direkten Kinder von <VISUALXML>.
    let mut children_stack: Vec<Vec<Node>> = vec![Vec::new()];
    let mut group_ids: Vec<String> = Vec::new();

    loop {
        match reader.read_event()? {
            Event::Start(e) => match e.name().as_ref() {
                b"VISUALXML" => {}
                b"GROUP" => {
                    group_ids.push(get_attr(&e, b"ID")?);
                    children_stack.push(Vec::new());
                }
                b"ITEM" => {
                    let item = parse_item(&mut reader, &e)?;
                    push_node(&mut children_stack, item);
                }
                other => {
                    return Err(XmlError::UnknownElement(
                        String::from_utf8_lossy(other).into_owned(),
                    ))
                }
            },
            Event::Empty(e) => match e.name().as_ref() {
                b"GROUP" => {
                    let id = get_attr(&e, b"ID")?;
                    push_node(
                        &mut children_stack,
                        Node::Group {
                            id,
                            children: Vec::new(),
                        },
                    );
                }
                b"ITEM" => {
                    let item = build_item(&e, String::new(), None)?;
                    push_node(&mut children_stack, item);
                }
                other => {
                    return Err(XmlError::UnknownElement(
                        String::from_utf8_lossy(other).into_owned(),
                    ))
                }
            },
            Event::End(e) if e.name().as_ref() == b"GROUP" => {
                let children = children_stack.pop().expect("GROUP-Stack nie leer");
                let id = group_ids.pop().expect("GROUP-ID-Stack nie leer");
                push_node(&mut children_stack, Node::Group { id, children });
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(children_stack.pop().expect("Stack-Basis bleibt erhalten"))
}

fn push_node(stack: &mut [Vec<Node>], node: Node) {
    stack.last_mut().expect("Stack nie leer").push(node);
}

/// Liest ein `ITEM` ab dem Start-Tag bis zum passenden End-Tag.
fn parse_item(reader: &mut Reader<&[u8]>, start: &BytesStart) -> Result<Node, XmlError> {
    let mut text = String::new();
    let mut datetime: Option<DateTime> = None;
    loop {
        // quick-xml liefert Entity-Referenzen als eigene GeneralRef-Events,
        // Text-Events enthalten daher reinen Text.
        match reader.read_event()? {
            Event::Text(t) => {
                text.push_str(&t.decode().map_err(quick_xml::Error::from)?);
            }
            Event::GeneralRef(r) => {
                text.push_str(&resolve_entity(&r)?);
            }
            Event::Empty(e) if e.name().as_ref() == b"DATETIME" => {
                datetime = Some(parse_datetime(&e)?);
            }
            Event::End(e) if e.name().as_ref() == b"ITEM" => break,
            Event::Eof => return Err(XmlError::UnexpectedEof),
            _ => {}
        }
    }
    build_item(start, text, datetime)
}

fn build_item(
    start: &BytesStart,
    text: String,
    datetime: Option<DateTime>,
) -> Result<Node, XmlError> {
    let id = get_attr(start, b"ID")?;
    let type_name = get_attr(start, b"TYPE")?;
    let value = match type_name.as_str() {
        "String" => Value::String(text),
        "Integer" => Value::Integer(parse_num(text.trim(), "Integer")?),
        "Float" => Value::Float(parse_num(text.trim(), "Float")?),
        "Bool" => Value::Bool(text.trim().eq_ignore_ascii_case("true")),
        "DateTime" => Value::DateTime(datetime.unwrap_or_default()),
        other => return Err(XmlError::UnknownType(other.to_string())),
    };
    Ok(Node::Item { id, value })
}

fn parse_datetime(e: &BytesStart) -> Result<DateTime, XmlError> {
    let mut dt = DateTime::default();
    for attr in e.attributes() {
        let attr = attr.map_err(quick_xml::Error::from)?;
        let raw = attr.normalized_value(XmlVersion::Implicit1_0)?;
        match attr.key.as_ref() {
            b"Y" => dt.year = parse_num(raw.trim(), "DATETIME/Y")?,
            b"MM" => dt.month = parse_num(raw.trim(), "DATETIME/MM")?,
            b"D" => dt.day = parse_num(raw.trim(), "DATETIME/D")?,
            b"H" => dt.hour = parse_num(raw.trim(), "DATETIME/H")?,
            b"M" => dt.minute = parse_num(raw.trim(), "DATETIME/M")?,
            b"S" => dt.second = parse_num(raw.trim(), "DATETIME/S")?,
            b"MS" => dt.millis = parse_num(raw.trim(), "DATETIME/MS")?,
            _ => {}
        }
    }
    Ok(dt)
}

/// Löst eine Entity-Referenz auf: numerisch (`&#223;`) oder eine der fünf
/// in XML vordefinierten Entities.
fn resolve_entity(r: &BytesRef) -> Result<String, XmlError> {
    if let Some(ch) = r.resolve_char_ref()? {
        return Ok(ch.to_string());
    }
    let name = r.decode().map_err(quick_xml::Error::from)?;
    let resolved = match name.as_ref() {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        other => return Err(XmlError::UnknownEntity(other.to_string())),
    };
    Ok(resolved.to_string())
}

fn parse_num<T: std::str::FromStr>(raw: &str, ty: &'static str) -> Result<T, XmlError> {
    raw.parse().map_err(|_| XmlError::InvalidValue {
        ty,
        raw: raw.to_string(),
    })
}

fn get_attr(e: &BytesStart, name: &[u8]) -> Result<String, XmlError> {
    for attr in e.attributes() {
        let attr = attr.map_err(quick_xml::Error::from)?;
        if attr.key.as_ref() == name {
            return Ok(attr.normalized_value(XmlVersion::Implicit1_0)?.into_owned());
        }
    }
    Err(XmlError::MissingAttribute(
        String::from_utf8_lossy(name).into_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(id: &str, children: Vec<Node>) -> Node {
        Node::Group {
            id: id.to_string(),
            children,
        }
    }

    fn item(id: &str, value: Value) -> Node {
        Node::Item {
            id: id.to_string(),
            value,
        }
    }

    #[test]
    fn encode_simple_group() {
        let nodes = vec![group(
            "Action",
            vec![item("ID", Value::String("LOGIN".into()))],
        )];
        assert_eq!(
            encode(&nodes),
            r#"<?xml version="1.0" encoding="UTF-8"?><VISUALXML VERSION="1.0">"#.to_string()
                + r#"<GROUP ID="Action"><ITEM ID="ID" TYPE="String">LOGIN</ITEM></GROUP>"#
                + "</VISUALXML>"
        );
    }

    #[test]
    fn encode_escapes_special_chars() {
        let nodes = vec![item("name", Value::String("Rot & Weiß <e.V.>".into()))];
        let xml = encode(&nodes);
        assert!(xml.contains("Rot &amp; Weiß &lt;e.V.&gt;"));
    }

    #[test]
    fn decode_all_value_types() {
        let xml = r#"<VISUALXML VERSION="1.0">
            <ITEM ID="s" TYPE="String">hallo</ITEM>
            <ITEM ID="i" TYPE="Integer">42</ITEM>
            <ITEM ID="f" TYPE="Float">1.5</ITEM>
            <ITEM ID="b" TYPE="Bool">true</ITEM>
        </VISUALXML>"#;
        let nodes = decode(xml).unwrap();
        assert_eq!(nodes[0], item("s", Value::String("hallo".into())));
        assert_eq!(nodes[1], item("i", Value::Integer(42)));
        assert_eq!(nodes[2], item("f", Value::Float(1.5)));
        assert_eq!(nodes[3], item("b", Value::Bool(true)));
    }

    #[test]
    fn decode_datetime_spec_example() {
        // Beispiel aus docs/btp_protocol.md (Timestamp 1652529397790).
        let xml = r#"<VISUALXML VERSION="1.0"><ITEM TYPE="DateTime" ID="test_date">"#.to_string()
            + r#"<DATETIME Y="2022" MM="5" D="14" H="13" M="56" S="37" MS="790"/>"#
            + r#"</ITEM></VISUALXML>"#;
        let nodes = decode(&xml).unwrap();
        assert_eq!(
            nodes[0],
            item(
                "test_date",
                Value::DateTime(DateTime {
                    year: 2022,
                    month: 5,
                    day: 14,
                    hour: 13,
                    minute: 56,
                    second: 37,
                    millis: 790,
                })
            )
        );
    }

    #[test]
    fn decode_nested_groups_and_repeated_ids() {
        // Zwei gleichnamige GROUPs = Liste mit zwei Einträgen.
        let xml = r#"<VISUALXML VERSION="1.0"><GROUP ID="Matches">
            <GROUP ID="Match"><ITEM ID="ID" TYPE="Integer">1</ITEM></GROUP>
            <GROUP ID="Match"><ITEM ID="ID" TYPE="Integer">2</ITEM></GROUP>
        </GROUP></VISUALXML>"#;
        let nodes = decode(xml).unwrap();
        let Node::Group { id, children } = &nodes[0] else {
            panic!("GROUP erwartet");
        };
        assert_eq!(id, "Matches");
        assert_eq!(children.len(), 2);
        assert_eq!(
            children[0],
            group("Match", vec![item("ID", Value::Integer(1))])
        );
        assert_eq!(
            children[1],
            group("Match", vec![item("ID", Value::Integer(2))])
        );
    }

    #[test]
    fn decode_resolves_entities() {
        let xml = r#"<VISUALXML VERSION="1.0"><ITEM ID="club" TYPE="String">Rot &amp; Wei&#223;</ITEM></VISUALXML>"#;
        let nodes = decode(xml).unwrap();
        assert_eq!(nodes[0], item("club", Value::String("Rot & Weiß".into())));
    }

    #[test]
    fn decode_empty_string_item() {
        let xml = r#"<VISUALXML VERSION="1.0"><ITEM ID="x" TYPE="String"/></VISUALXML>"#;
        let nodes = decode(xml).unwrap();
        assert_eq!(nodes[0], item("x", Value::String(String::new())));
    }

    #[test]
    fn decode_rejects_unknown_type() {
        let xml = r#"<VISUALXML VERSION="1.0"><ITEM ID="x" TYPE="Blob">y</ITEM></VISUALXML>"#;
        let err = decode(xml).unwrap_err();
        assert!(matches!(err, XmlError::UnknownType(t) if t == "Blob"));
    }

    #[test]
    fn roundtrip_request_like_tree() {
        let nodes = vec![
            group(
                "Header",
                vec![group(
                    "Version",
                    vec![item("Hi", Value::Integer(1)), item("Lo", Value::Integer(1))],
                )],
            ),
            group(
                "Action",
                vec![
                    item("ID", Value::String("LOGIN".into())),
                    item("Password", Value::String("geheim & sicher".into())),
                ],
            ),
            group(
                "Client",
                vec![item("IP", Value::String("bts-light".into()))],
            ),
        ];
        let decoded = decode(&encode(&nodes)).unwrap();
        assert_eq!(decoded, nodes);
    }
}
