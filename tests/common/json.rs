//! Minimal JSON reader for the test case files, so the crate builds and
//! tests without any external dependency (and therefore offline).
//!
//! Accepts standard JSON: objects, arrays, strings with escapes, numbers,
//! `true`, `false` and `null`.

use std::collections::BTreeMap;

use super::TestCase;

#[derive(Debug)]
enum Value {
    Null,
    Bool,
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn err<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("{what} at byte {}", self.pos))
    }

    fn ws(&mut self) {
        while self.pos < self.s.len() && self.s[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn eat(&mut self, b: u8) -> Result<(), String> {
        self.ws();
        if self.s.get(self.pos) == Some(&b) {
            self.pos += 1;
            Ok(())
        } else {
            self.err(&format!("expected '{}'", b as char))
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.ws();
        match self.s.get(self.pos) {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(Value::String),
            Some(b't') => self.literal("true", Value::Bool),
            Some(b'f') => self.literal("false", Value::Bool),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => self.err("expected a value"),
        }
    }

    fn literal(&mut self, word: &str, v: Value) -> Result<Value, String> {
        if self.s[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(v)
        } else {
            self.err("invalid literal")
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        while self.pos < self.s.len()
            && matches!(
                self.s[self.pos],
                b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'
            )
        {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.s[start..self.pos]).unwrap();
        text.parse()
            .map(Value::Number)
            .or_else(|_| self.err("invalid number"))
    }

    fn string(&mut self) -> Result<String, String> {
        self.eat(b'"')?;
        let mut out = Vec::new();
        loop {
            let Some(&b) = self.s.get(self.pos) else {
                return self.err("unterminated string");
            };
            self.pos += 1;
            match b {
                b'"' => break,
                b'\\' => {
                    let Some(&e) = self.s.get(self.pos) else {
                        return self.err("unterminated escape");
                    };
                    self.pos += 1;
                    let c = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hex = self.s.get(self.pos..self.pos + 4).and_then(|h| {
                                u32::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok()
                            });
                            self.pos += 4;
                            match hex.and_then(char::from_u32) {
                                Some(c) => c,
                                None => return self.err("unsupported \\u escape"),
                            }
                        }
                        _ => return self.err("invalid escape"),
                    };
                    let mut buf = [0; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                _ => out.push(b),
            }
        }
        String::from_utf8(out).or_else(|_| self.err("invalid UTF-8"))
    }

    fn array(&mut self) -> Result<Value, String> {
        self.eat(b'[')?;
        let mut items = Vec::new();
        self.ws();
        if self.s.get(self.pos) == Some(&b']') {
            self.pos += 1;
            return Ok(Value::Array(items));
        }
        loop {
            items.push(self.value()?);
            self.ws();
            match self.s.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Array(items));
                }
                _ => return self.err("expected ',' or ']'"),
            }
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.eat(b'{')?;
        let mut fields = BTreeMap::new();
        self.ws();
        if self.s.get(self.pos) == Some(&b'}') {
            self.pos += 1;
            return Ok(Value::Object(fields));
        }
        loop {
            self.ws();
            let key = self.string()?;
            self.eat(b':')?;
            fields.insert(key, self.value()?);
            self.ws();
            match self.s.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Object(fields));
                }
                _ => return self.err("expected ',' or '}'"),
            }
        }
    }
}

fn parse(text: &str) -> Result<Value, String> {
    let mut p = Parser {
        s: text.as_bytes(),
        pos: 0,
    };
    let v = p.value()?;
    p.ws();
    if p.pos != p.s.len() {
        return p.err("trailing characters");
    }
    Ok(v)
}

/// Parses an array of `{"testID", "t1", "t2", "d"}` objects.
pub fn parse_cases(text: &str) -> Result<Vec<TestCase>, String> {
    let Value::Array(items) = parse(text)? else {
        return Err("expected a top-level array".into());
    };
    items
        .into_iter()
        .map(|item| {
            let Value::Object(mut f) = item else {
                return Err("expected an object".into());
            };
            let mut int = |k: &str| match f.remove(k) {
                Some(Value::Number(n)) if n.fract() == 0.0 => Ok(n as i32),
                other => Err(format!("{k}: expected an integer, got {other:?}")),
            };
            let (test_id, d) = (int("testID")?, int("d")?);
            let mut string = |k: &str| match f.remove(k) {
                Some(Value::String(s)) => Ok(s),
                other => Err(format!("{k}: expected a string, got {other:?}")),
            };
            Ok(TestCase {
                test_id,
                t1: string("t1")?,
                t2: string("t2")?,
                d,
            })
        })
        .collect()
}
