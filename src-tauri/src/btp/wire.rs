//! TP-Network Wire-Layer: 4-Byte-Frame + gzip.
//!
//! Eine Wire-Nachricht ist `[4-Byte-BE-Längenheader][gzip(xml)]`.
//! Siehe `docs/btp_protocol.md`.

use std::io::{Read, Write};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

/// Länge des Frame-Headers in Bytes.
const HEADER_LEN: usize = 4;

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("Frame zu kurz: {0} Bytes (mindestens 4 erwartet)")]
    TooShort(usize),
    #[error("gzip-Dekompression fehlgeschlagen: {0}")]
    Gunzip(#[source] std::io::Error),
    #[error("Payload ist kein gültiges UTF-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}

/// Komprimiert Daten mit gzip (Standard-Wrapper, Magic `1f 8b`).
fn gzip_compress(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(data)
        .expect("Schreiben in einen Vec kann nicht fehlschlagen");
    encoder
        .finish()
        .expect("Finalisieren in einen Vec kann nicht fehlschlagen")
}

/// Dekomprimiert gzip-Daten.
fn gzip_decompress(data: &[u8]) -> Result<Vec<u8>, WireError> {
    let mut decoder = GzDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).map_err(WireError::Gunzip)?;
    Ok(out)
}

/// XML-String → fertige Wire-Nachricht (4-Byte-BE-Längenheader + gzip(xml)).
pub fn encode_message(xml: &str) -> Vec<u8> {
    let payload = gzip_compress(xml.as_bytes());
    let mut frame = Vec::with_capacity(HEADER_LEN + payload.len());
    frame.extend_from_slice(&(payload.len() as i32).to_be_bytes());
    frame.extend_from_slice(&payload);
    frame
}

/// Wire-Nachricht → XML-String.
///
/// Tolerant gegenüber falschem Längenwert: BTP sendet gelegentlich eine
/// fehlerhafte Länge, deshalb wird dem tatsächlich empfangenen Byte-Count
/// vertraut (siehe `docs/btp_protocol.md`).
pub fn decode_message(bytes: &[u8]) -> Result<String, WireError> {
    if bytes.len() < HEADER_LEN {
        return Err(WireError::TooShort(bytes.len()));
    }
    let declared = i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let payload = &bytes[HEADER_LEN..];
    if declared < 0 || declared as usize != payload.len() {
        tracing::warn!(
            declared,
            actual = payload.len(),
            "BTP-Frame: Längenheader weicht vom tatsächlichen Payload ab"
        );
    }
    let xml = gzip_decompress(payload)?;
    Ok(String::from_utf8(xml)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gzip_roundtrip() {
        let original = b"<VISUALXML VERSION=\"1.0\"></VISUALXML>";
        let compressed = gzip_compress(original);
        assert_eq!(&compressed[0..2], &[0x1f, 0x8b], "gzip-Magic erwartet");
        let restored = gzip_decompress(&compressed).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn encode_message_has_be_length_header() {
        let xml = "<VISUALXML VERSION=\"1.0\"/>";
        let frame = encode_message(xml);
        // i32::from_be_bytes liest den Header korrekt nur, wenn er Big-Endian
        // geschrieben wurde – dieser Test deckt damit auch die Byte-Reihenfolge ab.
        let declared = i32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]);
        assert_eq!(declared as usize, frame.len() - HEADER_LEN);
        assert_eq!(
            &frame[HEADER_LEN..HEADER_LEN + 2],
            &[0x1f, 0x8b],
            "Payload muss mit gzip-Magic beginnen"
        );
    }

    #[test]
    fn message_roundtrip() {
        let xml = "<?xml version=\"1.0\"?><VISUALXML VERSION=\"1.0\">\
                   <GROUP ID=\"Header\"/></VISUALXML>";
        let frame = encode_message(xml);
        assert_eq!(decode_message(&frame).unwrap(), xml);
    }

    #[test]
    fn decode_tolerates_wrong_declared_length() {
        let xml = "<VISUALXML VERSION=\"1.0\"/>";
        let mut frame = encode_message(xml);
        // Längenheader absichtlich verfälschen.
        frame[0] = 0x7f;
        frame[3] = 0x01;
        // Decode gelingt trotzdem, weil dem tatsächlichen Payload vertraut wird.
        assert_eq!(decode_message(&frame).unwrap(), xml);
    }

    #[test]
    fn decode_rejects_too_short() {
        let err = decode_message(&[0x00, 0x01]).unwrap_err();
        assert!(matches!(err, WireError::TooShort(2)));
    }

    #[test]
    fn decode_rejects_corrupt_gzip() {
        // Gültiger Header, aber Payload ist kein gzip.
        let frame = [0, 0, 0, 4, b'n', b'o', b'p', b'e'];
        let err = decode_message(&frame).unwrap_err();
        assert!(matches!(err, WireError::Gunzip(_)));
    }
}
