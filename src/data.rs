//! In-memory tables with small CSV inputs for reproducible demos.

use std::collections::{BTreeMap, HashSet};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    Int(i64),
    Text(String),
    Null,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(n) => write!(f, "{n}"),
            Self::Text(s) => write!(f, "{s}"),
            Self::Null => write!(f, "NULL"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Table {
    pub name: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

impl Table {
    pub fn from_csv(name: &str, input: &str) -> Result<Self, String> {
        let mut lines = input.lines().filter(|l| !l.trim().is_empty());
        let headers = csv_fields(lines.next().ok_or("CSV is empty")?)?;
        if headers.is_empty() || headers.iter().any(|c| c.trim().is_empty()) {
            return Err(format!("{name}: CSV requires nonempty column names"));
        }
        let columns: Vec<String> = headers.into_iter().map(|s| s.trim().to_string()).collect();
        let mut names = HashSet::new();
        if columns.iter().any(|c| !names.insert(c.to_ascii_lowercase())) {
            return Err(format!("{name}: duplicate CSV column"));
        }
        let mut rows = Vec::new();
        for (line_no, line) in lines.enumerate() {
            let fields = csv_fields(line)?;
            if fields.len() != columns.len() {
                return Err(format!(
                    "{name}: row {} has {} cells; expected {}",
                    line_no + 2,
                    fields.len(),
                    columns.len()
                ));
            }
            rows.push(fields.into_iter().map(|cell| {
                let cell = cell.trim();
                if cell.is_empty() {
                    Value::Null
                } else if let Ok(n) = cell.parse::<i64>() {
                    Value::Int(n)
                } else {
                    Value::Text(cell.to_string())
                }
            }).collect());
        }
        Ok(Self {
            name: name.to_string(),
            columns,
            rows,
        })
    }
}

// RFC-style quoted commas and doubled quotes. This demo intentionally does not
// support embedded newlines in quoted fields.
fn csv_fields(line: &str) -> Result<Vec<String>, String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;
    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(std::mem::take(&mut field));
            }
            _ => field.push(c),
        }
    }
    if in_quotes {
        return Err("unterminated quoted CSV field".into());
    }
    fields.push(field);
    Ok(fields)
}

#[derive(Clone, Debug, Default)]
pub struct Database {
    pub tables: BTreeMap<String, Table>,
}

impl Database {
    pub fn insert(&mut self, table: Table) {
        self.tables.insert(table.name.clone(), table);
    }

    pub fn table(&self, name: &str) -> Result<&Table, String> {
        self.tables.get(name).ok_or_else(|| format!("unknown table: {name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_csv_and_quoted_commas() {
        let table = Table::from_csv("people", "id,name\n1,\"Doe, Jane\"\n2,\"O\"\"Brien\"\n").unwrap();
        assert_eq!(table.rows[0][1], Value::Text("Doe, Jane".into()));
        assert_eq!(table.rows[1][1], Value::Text("O\"Brien".into()));
        assert!(Table::from_csv("bad", "id,name\n1\n").is_err());
        assert!(Table::from_csv("bad", "id,id\n1,2").is_err());
    }
}
