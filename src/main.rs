use miniquery::data::{Database, Table};
use miniquery::{compile, engine};
use std::env;
use std::fs;
use std::path::Path;
use std::time::Instant;

const DEFAULT_QUERY: &str = "SELECT customers.name, orders.amount FROM customers JOIN orders ON customers.id = orders.customer_id WHERE orders.amount >= 50 AND customers.region = 'US'";

fn usage() {
    eprintln!("Usage: miniquery [--query SQL] [--data-dir DIR] [--explain] [--baseline] [--compare N]");
    eprintln!("  --explain    Print execution plan");
    eprintln!("  --baseline   Disable predicate pushdown and projection pruning");
    eprintln!("  --compare N  Execute N runs of baseline and optimized plans; show timings");
}

fn load_tables(folder: &str) -> Result<Database, String> {
    let mut db = Database::default();
    let path = Path::new(folder);
    for entry in fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let file = entry.path();
        if file.extension().and_then(|s| s.to_str()) != Some("csv") {
            continue;
        }
        let name = file.file_stem().and_then(|s| s.to_str())
            .ok_or("invalid UTF-8 CSV filename")?;
        let text = fs::read_to_string(&file).map_err(|e| e.to_string())?;
        db.insert(Table::from_csv(name, &text)?);
    }
    if db.tables.is_empty() {
        return Err(format!("no .csv tables found in {}", path.display()));
    }
    Ok(db)
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let mut sql = DEFAULT_QUERY.to_string();
    let mut data_dir = "examples/data".to_string();
    let mut explain = false;
    let mut baseline = false;
    let mut compare = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--query" => sql = args.next().ok_or("--query requires SQL")?,
            "--data-dir" => data_dir = args.next().ok_or("--data-dir requires a directory")?,
            "--explain" => explain = true,
            "--baseline" => baseline = true,
            "--compare" => {
                let runs: usize = args.next().ok_or("--compare requires N")?
                    .parse().map_err(|_| "invalid --compare run count")?;
                if runs == 0 || runs > 1_000_000 {
                    return Err("--compare must be in 1..=1000000".into());
                }
                compare = Some(runs);
            }
            "-h" | "--help" => {
                usage();
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }

    let db = load_tables(&data_dir)?;
    let plan = compile(&db, &sql, !baseline)?;
    if explain {
        println!("Execution plan:\n{}", plan.explain());
    }
    let result = engine::execute(&db, &plan)?;
    println!("{}", result.columns.join(" | "));
    for row in &result.rows {
        let values: Vec<String> = row.iter().map(ToString::to_string).collect();
        println!("{}", values.join(" | "));
    }
    println!("{} row(s)", result.rows.len());

    if let Some(runs) = compare {
        for (label, optimize) in [("baseline", false), ("optimized", true)] {
            let p = compile(&db, &sql, optimize)?;
            let started = Instant::now();
            let mut row_count = 0;
            for _ in 0..runs {
                row_count = engine::execute(&db, &p)?.rows.len();
            }
            let elapsed = started.elapsed();
            println!("{label}: {runs} runs, {row_count} output rows, total {elapsed:?}, average {:.2} us", elapsed.as_secs_f64() * 1_000_000.0 / runs as f64);
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("miniquery: {error}");
        std::process::exit(1);
    }
}
