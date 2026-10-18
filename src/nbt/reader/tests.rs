use std::io::Write;

use flate2::write::GzEncoder;
use flate2::Compression;

use crate::nbt::error::Error;
use crate::nbt::value::Value;

use super::{read, read_gzip};

const ALL_TAGS: [u8; 151] = [
    0x0a, 0x00, 0x00,
    0x01, 0x00, 0x01, 0x61, 0x07,
    0x02, 0x00, 0x01, 0x62, 0x01, 0x02,
    0x03, 0x00, 0x01, 0x63, 0x00, 0x01, 0x02, 0x03,
    0x04, 0x00, 0x01, 0x64, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
    0x05, 0x00, 0x01, 0x65, 0x41, 0x20, 0x00, 0x00,
    0x06, 0x00, 0x01, 0x66, 0x40, 0x24, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x07, 0x00, 0x01, 0x67, 0x00, 0x00, 0x00, 0x03, 0x01, 0x02, 0x03,
    0x08, 0x00, 0x01, 0x68, 0x00, 0x05, 0x68, 0x65, 0x6c, 0x6c, 0x6f,
    0x0b, 0x00, 0x01, 0x69, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03,
    0x0c, 0x00, 0x01, 0x6a, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
    0x09, 0x00, 0x01, 0x6b, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x0b,
    0x0a, 0x00, 0x01, 0x6c, 0x03, 0x00, 0x01, 0x6d, 0x00, 0x00, 0x00, 0x2a, 0x00,
    0x00,
];

const LIST_OF_COMPOUNDS: [u8; 31] = [
    0x0a, 0x00, 0x00,
    0x09, 0x00, 0x01, 0x6c, 0x0a, 0x00, 0x00, 0x00, 0x02,
    0x03, 0x00, 0x01, 0x6e, 0x00, 0x00, 0x00, 0x01, 0x00,
    0x03, 0x00, 0x01, 0x6e, 0x00, 0x00, 0x00, 0x02, 0x00,
    0x00,
];

const GZIP_SOURCE: [u8; 21] = [
    0x0a, 0x00, 0x00,
    0x08, 0x00, 0x05, 0x77, 0x6f, 0x72, 0x6c, 0x64, 0x00, 0x07, 0x67, 0x7a, 0x69, 0x70, 0x70, 0x65, 0x64,
    0x00,
];

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).expect("write");
    encoder.finish().expect("finish")
}

fn expected_all_tags() -> Value {
    Value::Compound(vec![
        ("a".into(), Value::Byte(7)),
        ("b".into(), Value::Short(0x0102)),
        ("c".into(), Value::Int(0x00010203)),
        ("d".into(), Value::Long(0x0001020304050607)),
        ("e".into(), Value::Float(10.0)),
        ("f".into(), Value::Double(10.0)),
        ("g".into(), Value::ByteArray(vec![1, 2, 3])),
        ("h".into(), Value::String("hello".into())),
        ("i".into(), Value::IntArray(vec![1, 2, 3])),
        ("j".into(), Value::LongArray(vec![1, 2])),
        (
            "k".into(),
            Value::List {
                element: 3,
                items: vec![Value::Int(10), Value::Int(11)],
            },
        ),
        ("l".into(), Value::Compound(vec![("m".into(), Value::Int(42))])),
    ])
}

#[test]
fn reads_all_tag_types() {
    assert_eq!(read(&ALL_TAGS).expect("all tags"), expected_all_tags());
}

#[test]
fn field_accessors() {
    let value = read(&ALL_TAGS).expect("all tags");
    assert_eq!(value.as_compound().map(|fields| fields.len()), Some(12));
    assert_eq!(value.get("c").and_then(Value::as_i32), Some(0x00010203));
    assert_eq!(value.get("d").and_then(Value::as_i64), Some(0x0001020304050607));
    assert_eq!(value.get("f").and_then(Value::as_f64), Some(10.0));
    assert_eq!(value.get("h").and_then(Value::as_str), Some("hello"));
    assert_eq!(value.get("g").and_then(Value::as_bytes), Some([1i8, 2, 3].as_slice()));
    assert_eq!(value.get("k").and_then(Value::as_list).map(|items| items.len()), Some(2));
    assert!(value.get("q").is_none());
}

#[test]
fn reads_list_of_compounds() {
    let expected = Value::Compound(vec![(
        "l".into(),
        Value::List {
            element: 10,
            items: vec![
                Value::Compound(vec![("n".into(), Value::Int(1))]),
                Value::Compound(vec![("n".into(), Value::Int(2))]),
            ],
        },
    )]);
    assert_eq!(read(&LIST_OF_COMPOUNDS).expect("list of compounds"), expected);
}

#[test]
fn reads_gzipped_compound() {
    let compressed = gzip(&GZIP_SOURCE);
    let expected = Value::Compound(vec![("world".into(), Value::String("gzipped".into()))]);
    assert_eq!(read_gzip(&compressed).expect("gzip"), expected);
}

#[test]
fn rejects_unknown_type() {
    let bytes = [0x2a, 0x00, 0x00];
    assert_eq!(read(&bytes).expect_err("unknown type"), Error::UnknownType(0x2a));
}

#[test]
fn rejects_truncated_payload() {
    let bytes = [0x0a, 0x00, 0x00, 0x03, 0x00, 0x01, 0x61, 0x00, 0x01];
    assert_eq!(read(&bytes).expect_err("truncated"), Error::Truncated);
}

#[test]
fn rejects_truncated_string_length() {
    let bytes = [0x08, 0x00, 0x01, 0x73, 0x00, 0x05, 0x68, 0x65];
    assert_eq!(read(&bytes).expect_err("truncated"), Error::Truncated);
}

#[test]
fn rejects_invalid_utf8() {
    let bytes = [0x08, 0x00, 0x01, 0x73, 0x00, 0x02, 0xff, 0xfe];
    assert_eq!(read(&bytes).expect_err("utf8"), Error::InvalidUtf8);
}

#[test]
fn rejects_negative_list_count() {
    let bytes = [0x0a, 0x00, 0x00, 0x09, 0x00, 0x01, 0x6c, 0x03, 0xff, 0xff, 0xff, 0xff];
    assert_eq!(read(&bytes).expect_err("count"), Error::InvalidCount);
}

#[test]
fn rejects_bad_gzip() {
    assert_eq!(read_gzip(b"not gzip").expect_err("gzip"), Error::InvalidGzip);
}

#[test]
fn rejects_empty_input() {
    assert_eq!(read(&[]).expect_err("empty"), Error::Truncated);
}
