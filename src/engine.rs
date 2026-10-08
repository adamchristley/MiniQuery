//! Executable operators: table scan, filter, hash join, projection, and limit.

use crate::data::{Database, Value};
use crate::plan::Plan;
use crate::sql::{Literal, Operand, Operator, Predicate};
use std::cmp::Ordering;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultSet {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

fn column_index(columns: &[String], name: &str) -> Result<usize, String> {
    columns.iter()
        .position(|column| column == name)
        .ok_or_else(|| format!("execution column not found: {name}"))
}

fn compare_values(left: &Value, right: &Value, op: Operator) -> bool {
    let order = match (left, right) {
        (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
        (Value::Text(a), Value::Text(b)) => Some(a.cmp(b)),
        _ => None,
    };
    match order {
        None => false, // NULL and cross-type comparisons do not match.
        Some(order) => match op {
            Operator::Eq => order == Ordering::Equal,
            Operator::Ne => order != Ordering::Equal,
            Operator::Lt => order == Ordering::Less,
            Operator::Le => order != Ordering::Greater,
            Operator::Gt => order == Ordering::Greater,
            Operator::Ge => order != Ordering::Less,
        },
    }
}

enum RightOperand {
    Column(usize),
    Literal(Value),
}

struct CompiledPredicate {
    left: usize,
    op: Operator,
    right: RightOperand,
}

impl CompiledPredicate {
    fn compile(predicate: &Predicate, columns: &[String]) -> Result<Self, String> {
        let left = column_index(columns, &predicate.left.qualified())?;
        let right = match &predicate.right {
            Operand::Column(column) => {
                RightOperand::Column(column_index(columns, &column.qualified())?)
            }
            Operand::Literal(Literal::Integer(n)) => RightOperand::Literal(Value::Int(*n)),
            Operand::Literal(Literal::Text(s)) => RightOperand::Literal(Value::Text(s.clone())),
        };
        Ok(Self {
            left,
            op: predicate.op,
            right,
        })
    }

    fn matches(&self, row: &[Value]) -> bool {
        let right = match &self.right {
            RightOperand::Column(index) => &row[*index],
            RightOperand::Literal(value) => value,
        };
        compare_values(&row[self.left], right, self.op)
    }
}

pub fn execute(db: &Database, plan: &Plan) -> Result<ResultSet, String> {
    match plan {
        Plan::Scan { table, columns } => {
            let source = db.table(table)?;
            let indices = columns.iter().map(|c| {
                source.columns.iter().position(|src| src == c)
                    .ok_or_else(|| format!("{table}: unknown scan column {c}"))
            }).collect::<Result<Vec<_>, _>>()?;
            Ok(ResultSet {
                columns: columns.iter().map(|c| format!("{table}.{c}")).collect(),
                rows: source.rows.iter().map(|row| indices.iter().map(|i| row[*i].clone()).collect()).collect(),
            })
        }
        Plan::Filter { input, predicates } => {
            let mut result = execute(db, input)?;
            let compiled = predicates.iter()
                .map(|p| CompiledPredicate::compile(p, &result.columns))
                .collect::<Result<Vec<_>, _>>()?;
            result.rows.retain(|row| compiled.iter().all(|p| p.matches(row)));
            Ok(result)
        }
        Plan::HashJoin { left, right, left_key, right_key } => {
            let l = execute(db, left)?;
            let r = execute(db, right)?;
            let l_index = column_index(&l.columns, &left_key.qualified())?;
            let r_index = column_index(&r.columns, &right_key.qualified())?;
            let mut columns = l.columns.clone();
            columns.extend(r.columns.clone());
            let mut rows = Vec::new();

            // Build the hash table over the smaller input to reduce memory use.
            if l.rows.len() <= r.rows.len() {
                let mut lookup: HashMap<&Value, Vec<usize>> = HashMap::new();
                for (index, row) in l.rows.iter().enumerate() {
                    if row[l_index] != Value::Null {
                        lookup.entry(&row[l_index]).or_default().push(index);
                    }
                }
                for right_row in &r.rows {
                    if let Some(indices) = lookup.get(&right_row[r_index]) {
                        for &index in indices {
                            let mut joined = l.rows[index].clone();
                            joined.extend(right_row.iter().cloned());
                            rows.push(joined);
                        }
                    }
                }
            } else {
                let mut lookup: HashMap<&Value, Vec<usize>> = HashMap::new();
                for (index, row) in r.rows.iter().enumerate() {
                    if row[r_index] != Value::Null {
                        lookup.entry(&row[r_index]).or_default().push(index);
                    }
                }
                for left_row in &l.rows {
                    if let Some(indices) = lookup.get(&left_row[l_index]) {
                        for &index in indices {
                            let mut joined = left_row.clone();
                            joined.extend(r.rows[index].iter().cloned());
                            rows.push(joined);
                        }
                    }
                }
            }
            Ok(ResultSet { columns, rows })
        }
        Plan::Project { input, columns } => {
            let result = execute(db, input)?;
            let names: Vec<String> = columns.iter().map(|c| c.qualified()).collect();
            let indices = names.iter()
                .map(|name| column_index(&result.columns, name))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ResultSet {
                columns: names,
                rows: result.rows.iter().map(|row| indices.iter().map(|i| row[*i].clone()).collect()).collect(),
            })
        }
        Plan::Limit { input, count } => {
            let mut result = execute(db, input)?;
            result.rows.truncate(*count);
            Ok(result)
        }
    }
}
