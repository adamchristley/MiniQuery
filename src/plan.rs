//! Logical plan construction, predicate pushdown, and scan-column pruning.

use crate::data::{Database, Table};
use crate::sql::{Column, Join, Operand, Predicate, Query};
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub enum Plan {
    Scan {
        table: String,
        columns: Vec<String>,
    },
    Filter {
        input: Box<Plan>,
        predicates: Vec<Predicate>,
    },
    HashJoin {
        left: Box<Plan>,
        right: Box<Plan>,
        left_key: Column,
        right_key: Column,
    },
    Project {
        input: Box<Plan>,
        columns: Vec<Column>,
    },
    Limit {
        input: Box<Plan>,
        count: usize,
    },
}

impl Plan {
    pub fn explain(&self) -> String {
        let mut lines = String::new();
        self.explain_at(0, &mut lines);
        lines
    }

    fn explain_at(&self, depth: usize, output: &mut String) {
        let indent = "  ".repeat(depth);
        match self {
            Plan::Scan { table, columns } => {
                output.push_str(&format!("{indent}Scan {table} [{}]\n", columns.join(", ")));
            }
            Plan::Filter { input, predicates } => {
                output.push_str(&format!("{indent}Filter ({} predicate(s))\n", predicates.len()));
                input.explain_at(depth + 1, output);
            }
            Plan::HashJoin { left, right, left_key, right_key } => {
                output.push_str(&format!(
                    "{indent}HashJoin {} = {}\n",
                    left_key.qualified(),
                    right_key.qualified()
                ));
                left.explain_at(depth + 1, output);
                right.explain_at(depth + 1, output);
            }
            Plan::Project { input, columns } => {
                let names: Vec<String> = columns.iter().map(Column::qualified).collect();
                output.push_str(&format!("{indent}Project [{}]\n", names.join(", ")));
                input.explain_at(depth + 1, output);
            }
            Plan::Limit { input, count } => {
                output.push_str(&format!("{indent}Limit {count}\n"));
                input.explain_at(depth + 1, output);
            }
        }
    }
}

fn normalize_column(column: &Column, tables: &[&Table]) -> Result<Column, String> {
    let matches: Vec<(&Table, &String)> = tables
        .iter()
        .flat_map(|t| {
            t.columns
                .iter()
                .filter(move |name| {
                    name.eq_ignore_ascii_case(&column.name)
                        && column.table.as_ref().is_none_or(|prefix| prefix.eq_ignore_ascii_case(&t.name))
                })
                .map(move |name| (*t, name))
        })
        .collect();

    match matches.as_slice() {
        [] => Err(format!("unknown column: {}", column.qualified())),
        [(table, name)] => Ok(Column {
            table: Some(table.name.clone()),
            name: name.to_string(),
        }),
        _ => Err(format!("ambiguous column: {}", column.qualified())),
    }
}

fn normalize_predicate(predicate: &Predicate, tables: &[&Table]) -> Result<Predicate, String> {
    let right = match &predicate.right {
        Operand::Column(col) => Operand::Column(normalize_column(col, tables)?),
        Operand::Literal(literal) => Operand::Literal(literal.clone()),
    };
    Ok(Predicate {
        left: normalize_column(&predicate.left, tables)?,
        op: predicate.op,
        right,
    })
}

fn predicates_for_table(predicate: &Predicate, table: &str) -> bool {
    let left_matches = predicate.left.table.as_deref() == Some(table);
    let right_matches = match &predicate.right {
        Operand::Column(col) => col.table.as_deref() == Some(table),
        Operand::Literal(_) => true,
    };
    left_matches && right_matches
}

fn scan(table: &Table, optimize: bool, needed: &HashSet<String>) -> Plan {
    Plan::Scan {
        table: table.name.clone(),
        columns: table
            .columns
            .iter()
            .filter(|column| !optimize || needed.contains(&format!("{}.{}", table.name, column)))
            .cloned()
            .collect(),
    }
}

fn with_filter(input: Plan, predicates: Vec<Predicate>) -> Plan {
    if predicates.is_empty() {
        input
    } else {
        Plan::Filter {
            input: Box::new(input),
            predicates,
        }
    }
}

fn normalized_join(join: &Join, from: &Table, joined: &Table) -> Result<(Column, Column), String> {
    let tables = [from, joined];
    let a = normalize_column(&join.left, &tables)?;
    let b = normalize_column(&join.right, &tables)?;
    if a.table.as_deref() == Some(from.name.as_str()) && b.table.as_deref() == Some(joined.name.as_str()) {
        Ok((a, b))
    } else if b.table.as_deref() == Some(from.name.as_str()) && a.table.as_deref() == Some(joined.name.as_str()) {
        Ok((b, a))
    } else {
        Err("JOIN keys must reference opposite tables".into())
    }
}

/// Build a plan with or without safe relational rewrites.
/// The baseline scans every column and filters after joining; the optimized
/// plan pushes single-table filters below the join and prunes unused columns.
pub fn build(db: &Database, query: &Query, optimize: bool) -> Result<Plan, String> {
    let from = db.table(&query.from)?;
    let joined = query.join.as_ref().map(|j| db.table(&j.table)).transpose()?;
    if joined.is_some_and(|t| t.name == from.name) {
        return Err("self joins require aliases and are not supported".into());
    }

    let mut tables = vec![from];
    if let Some(table) = joined {
        tables.push(table);
    }

    let projection = if query.select_all {
        tables.iter().flat_map(|t| {
            t.columns.iter().map(|name| Column {
                table: Some(t.name.clone()),
                name: name.clone(),
            })
        }).collect()
    } else {
        query.projection.iter().map(|c| normalize_column(c, &tables)).collect::<Result<Vec<_>, _>>()?
    };
    let filters: Vec<Predicate> = query.filters.iter()
        .map(|p| normalize_predicate(p, &tables))
        .collect::<Result<_, _>>()?;

    let keys = match (&query.join, joined) {
        (Some(spec), Some(table)) => Some(normalized_join(spec, from, table)?),
        _ => None,
    };

    let mut needed = HashSet::new();
    for column in &projection {
        needed.insert(column.qualified());
    }
    for predicate in &filters {
        needed.insert(predicate.left.qualified());
        if let Operand::Column(column) = &predicate.right {
            needed.insert(column.qualified());
        }
    }
    if let Some((left, right)) = &keys {
        needed.insert(left.qualified());
        needed.insert(right.qualified());
    }

    let mut left_filters = Vec::new();
    let mut right_filters = Vec::new();
    let mut remaining = Vec::new();
    for p in filters {
        if optimize && predicates_for_table(&p, &from.name) {
            left_filters.push(p);
        } else if optimize && joined.is_some_and(|t| predicates_for_table(&p, &t.name)) {
            right_filters.push(p);
        } else {
            remaining.push(p);
        }
    }

    let mut plan = with_filter(scan(from, optimize, &needed), left_filters);
    if let (Some(table), Some((left_key, right_key))) = (joined, keys) {
        plan = Plan::HashJoin {
            left: Box::new(plan),
            right: Box::new(with_filter(scan(table, optimize, &needed), right_filters)),
            left_key,
            right_key,
        };
    }
    plan = with_filter(plan, remaining);
    plan = Plan::Project {
        input: Box::new(plan),
        columns: projection,
    };
    if let Some(count) = query.limit {
        plan = Plan::Limit { input: Box::new(plan), count };
    }
    Ok(plan)
}
