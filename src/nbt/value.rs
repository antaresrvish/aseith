#[derive(Debug, PartialEq)]
pub enum Value {
    End,
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List {
        element: u8,
        items: Vec<Value>,
    },
    Compound(Vec<(String, Value)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Value {
    pub fn get(&self, name: &str) -> Option<&Value> {
        match self {
            Value::Compound(fields) => {
                fields.iter().find(|(key, _)| key == name).map(|(_, value)| value)
            }
            _ => None,
        }
    }

    pub fn as_compound(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Compound(fields) => Some(fields.as_slice()),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List { items, .. } => Some(items.as_slice()),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[i8]> {
        match self {
            Value::ByteArray(bytes) => Some(bytes.as_slice()),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_i32(&self) -> Option<i32> {
        match self {
            Value::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Long(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Double(v) => Some(*v),
            _ => None,
        }
    }
}
