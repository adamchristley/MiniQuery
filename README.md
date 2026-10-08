# MiniQuery

**A dependency-free, in-memory relational query engine written in Rust.**

MiniQuery is an educational systems project that turns a small subset of SQL into an executable operator tree, performs relational optimization, and evaluates queries over CSV-backed tables. It is focused on the mechanisms behind database query planners: parsing, schema binding, query rewrites, hash joins, and performance measurement.

> MiniQuery is not a production database and makes no claim to implement the full SQL standard or distributed execution.

## Architecture

```text
SQL text
   |
   v
Tokenizer + parser  ->  typed SQL AST
   |
   v
Schema binding       ->  qualified column references and validation
   |
   v
Logical planner      ->  Scan / Filter / HashJoin / Project / Limit
   |
   v
Rule-based optimizer ->  predicate pushdown + projection pruning
   |
   v
Execution engine     ->  row-oriented scans + hash join + predicates
   |
   v
Result rows
```

### Implemented features

- **SQL compiler front-end:** tokenizer, parser, typed AST, case-insensitive keywords, schema validation, ambiguous-column errors.
- **Logical operator tree:** scan, filter, inner equijoin, project, limit.
- **Predicate pushdown:** move a WHERE condition below a join when every referenced column comes from one side.
- **Projection pruning:** only scan columns needed by the SELECT list, join keys, or WHERE predicates.
- **Hash join:** hash the smaller materialized side and probe with the other, preserving SQL NULL non-matching semantics.
- **Reproducibility:** deterministic fixture CSVs, SQL correctness tests, baseline/optimized equivalence tests, GitHub Actions CI, synthetic workload generator.
- **Plan inspection:** compare the logical trees emitted with and without optimization.

The execution engine runs entirely in-process. It does not implement multiple-worker distribution, compilation to machine code, or a cost-based join-order search. Those are future extensions, not current capabilities.

## Quick start

Install a recent stable Rust toolchain, then run in the repository root:

```bash
git clone https://github.com/adamchristley/MiniQuery.git
cd MiniQuery
cargo test
cargo run -- --explain
```

Default query:

```sql
SELECT customers.name, orders.amount
FROM customers
JOIN orders ON customers.id = orders.customer_id
WHERE orders.amount >= 50 AND customers.region = 'US';
```

The sample data lives in `examples/data/`.

## Run a custom query

```bash
cargo run -- --query "SELECT name FROM customers WHERE region = 'US' LIMIT 2"

cargo run -- --explain --query "SELECT customers.name FROM customers JOIN orders ON customers.id = orders.customer_id WHERE orders.amount >= 50"

cargo run -- --baseline --explain
```

### Compare query plans

Baseline execution scans all columns and evaluates WHERE conditions **after** the join. Optimized execution pushes eligible conditions down and scans only required fields.

With the default SQL, the optimized plan contains independently filtered customer and order scans before the hash join, while the baseline filters the combined joined rows. Query results should be equivalent, except that without ORDER BY, row ordering is not specified.

```bash
cargo run --release -- --explain --compare 100
```

`--compare N` measures N repeated baseline and optimized executions on the same in-memory tables, **excluding** file loading, SQL parsing, and plan construction. This isolates execution cost. For credible comparisons, use Release builds, repeat experiments, and report hardware and workload sizes; speedup is not guaranteed on tiny inputs.

## Larger reproducible benchmark

Create deterministic synthetic CSV data (requires Python 3):

```bash
python3 scripts/generate_data.py --customers 10000 --orders 100000 --seed 42 --output-dir benchdata

cargo run --release -- --data-dir benchdata --explain --compare 20 --query "SELECT customers.name, orders.amount FROM customers JOIN orders ON customers.id = orders.customer_id WHERE orders.amount >= 900 AND customers.region = 'US'"
```

Change the row counts and selectivity to study memory use, filter pushdown, and join work. The CLI reports elapsed time and average microseconds per execution. No fixed benchmark performance numbers are claimed.

## Supported SQL grammar

```sql
SELECT * | column [, column ...]
FROM table
[INNER] JOIN table ON column = column
WHERE column (= | != | <> | < | <= | > | >=) (column | integer | 'text')
  [AND ...]
LIMIT non_negative_integer;
```

`JOIN`, `WHERE`, and `LIMIT` are optional. Only **one** inner equijoin is currently supported. Columns may be qualified as `table.column`. Identifiers are case-insensitive at schema-binding time. No aliases, aggregates, OR, ORDER BY, GROUP BY, expressions, or arbitrary join predicates yet.

### Input/SQL semantics

- Tables are loaded from `*.csv` files in `--data-dir`; a filename becomes the table name.
- CSV rows are parsed into signed 64-bit integers, UTF-8 text, or NULL for empty cells. Quoted commas and doubled quotes are supported; **quoted embedded newlines are not**.
- Numeric and text comparisons use their respective types; mixed-type comparisons and comparisons involving NULL do not match.
- The engine reports unknown tables/columns, ambiguous unqualified columns, and unsupported SQL rather than guessing.
- The JOIN operator may change row order based on its chosen hash-build side. There is no ORDER BY.

## Test and project layout

```text
src/
  sql.rs          tokenizer, SQL AST, parser
  data.rs         CSV ingestion and typed rows
  plan.rs         schema binding and optimizer
  engine.rs       executable relational operators
  lib.rs          library API
  main.rs         query, EXPLAIN, benchmark CLI
tests/
  query_tests.rs  correctness and baseline/optimized equivalence
examples/data/    small example tables
scripts/          reproducible benchmark input generator
.github/workflows/ci.yml
```

```bash
cargo test --all-targets
```

## Engineering roadmap

1. Cost-based plan selection using table statistics and estimated cardinalities.
2. Multiway join ordering and selection of hash versus nested-loop joins.
3. Aggregate operators (`COUNT`, `SUM`, `GROUP BY`) and stronger SQL expressions.
4. Streaming/vectorized execution, memory accounting, and operator-level profiling.
5. Disk-backed columnar storage, parallel scans, and distributed worker execution.

## License

MIT. See `LICENSE`.
