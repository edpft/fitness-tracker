//! The export's CSV, read as records of fields.
//!
//! **Quoted fields hold line breaks.** A workout's description is several
//! lines inside one quoted field, so the file cannot be split on lines first,
//! which is what the spreadsheets' CSV reader does. RFC 4180 is small enough
//! to read here: a field is quoted or not, and `""` inside quotes is a quote.

/// Every record in the file, the header included, or why it is not CSV.
pub(super) fn records(bytes: &[u8]) -> Result<Vec<Vec<String>>, String> {
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => quoted = false,
                other => field.push(other),
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => quoted = true,
            ',' => record.push(std::mem::take(&mut field)),
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            other => field.push(other),
        }
    }
    if quoted {
        return Err("a quoted field is never closed".to_owned());
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::records;

    #[test]
    fn a_quoted_field_keeps_its_line_breaks_and_quotes() {
        let read = records(b"a,b\n1,\"two\nlines, \"\"quoted\"\"\"\n").expect("csv");
        assert_eq!(
            read,
            vec![
                vec!["a".to_owned(), "b".to_owned()],
                vec!["1".to_owned(), "two\nlines, \"quoted\"".to_owned()],
            ]
        );
    }

    #[test]
    fn an_unclosed_quote_is_not_csv() {
        assert!(records(b"a,\"b\n").is_err());
    }
}
