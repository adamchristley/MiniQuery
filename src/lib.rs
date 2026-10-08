//! MiniQuery: a small Rust query planner and in-memory relational engine.
//!
//! Supported subset: SELECT columns or *, FROM, one INNER JOIN, equality join
//! keys, AND-connected comparisons, and LIMIT. Optimizer rewrites are optional.
//! SQL is parsed into a typed AST, normalized against table schemas, then
//! compiled into an executable operator tree.

pub mod data;
pub mod engine;
pub mod plan;
pub mod sql;

use crate::data::Database;
use crate::engine::ResultSet;
use crate::plan::Plan;

pub fn compile(db: &Database, sql: &str, optimize: bool) -> Result<Plan, String> {
    let query = sql::parse(sql)?;
    plan::build(db, &query, optimize)
}

pub fn query(db: &Database, sql: &str, optimize: bool) -> Result<ResultSet, String> {
    let plan = compile(db, sql, optimize)?;
    engine::execute(db, &plan)
}
