use miniquery::data::{Database, Table, Value};
use miniquery::{compile, query};

fn fixture() -> Database {
    let mut db = Database::default();
    db.insert(Table::from_csv(
        "customers",
        "id,name,region\n1,Ada,US\n2,Bob,US\n3,Cleo,CA\n4,Diego,MX\n5,Eve,US\n",
    ).unwrap());
    db.insert(Table::from_csv(
        "orders",
        "id,customer_id,amount,status\n101,1,60,shipped\n102,1,20,pending\n103,2,80,shipped\n104,3,90,pending\n105,5,100,shipped\n106,4,15,pending\n",
    ).unwrap());
    db
}

fn sorted_rows(mut rows: Vec<Vec<Value>>) -> Vec<Vec<Value>> {
    rows.sort_by_key(|row| format!("{row:?}"));
    rows
}

#[test]
fn optimized_and_baseline_results_are_equivalent() {
    let db = fixture();
    let cases = [
        "SELECT name FROM customers WHERE region = 'US'",
        "SELECT * FROM customers WHERE id >= 2 AND id < 5",
        "SELECT customers.name, orders.amount FROM customers JOIN orders ON customers.id = orders.customer_id WHERE orders.amount >= 50 AND customers.region = 'US'",
        "SELECT name, amount FROM customers INNER JOIN orders ON customers.id = orders.customer_id WHERE orders.amount > 70",
        "SELECT customers.name FROM customers JOIN orders ON customers.id = orders.customer_id WHERE customers.id = orders.customer_id",
        "SELECT name FROM customers WHERE name != 'Bob'",
        "SELECT id FROM customers WHERE id < 0",
    ];
    for sql in cases {
        let baseline = query(&db, sql, false).unwrap();
        let optimized = query(&db, sql, true).unwrap();
        assert_eq!(baseline.columns, optimized.columns, "{sql}");
        assert_eq!(sorted_rows(baseline.rows), sorted_rows(optimized.rows), "{sql}");
    }
}

#[test]
fn filters_and_hash_join_return_expected_rows() {
    let db = fixture();
    let sql = "SELECT customers.name, orders.amount FROM customers JOIN orders ON customers.id = orders.customer_id WHERE orders.amount >= 50 AND customers.region = 'US'";
    let result = query(&db, sql, true).unwrap();
    assert_eq!(result.columns, ["customers.name", "orders.amount"]);
    assert_eq!(sorted_rows(result.rows), sorted_rows(vec![
        vec![Value::Text("Ada".into()), Value::Int(60)],
        vec![Value::Text("Bob".into()), Value::Int(80)],
        vec![Value::Text("Eve".into()), Value::Int(100)],
    ]));
}

#[test]
fn explain_shows_rewrites() {
    let db = fixture();
    let sql = "SELECT customers.name FROM customers JOIN orders ON customers.id = orders.customer_id WHERE orders.amount >= 50 AND customers.region = 'US'";
    let baseline = compile(&db, sql, false).unwrap().explain();
    let optimized = compile(&db, sql, true).unwrap().explain();
    assert!(baseline.contains("Scan customers [id, name, region]"));
    assert!(baseline.contains("Scan orders [id, customer_id, amount, status]"));
    assert!(optimized.contains("Scan customers [id, name, region]"));
    assert!(optimized.contains("Scan orders [customer_id, amount]"));
    assert!(optimized.matches("Filter (1 predicate(s))").count() == 2);
}

#[test]
fn handles_limit_zero_and_wildcard() {
    let db = fixture();
    let one = query(&db, "SELECT * FROM customers LIMIT 1", true).unwrap();
    assert_eq!(one.columns.len(), 3);
    assert_eq!(one.rows.len(), 1);
    let zero = query(&db, "SELECT id FROM customers LIMIT 0", true).unwrap();
    assert!(zero.rows.is_empty());
}

#[test]
fn rejects_ambiguous_unknown_and_unsupported_sql() {
    let db = fixture();
    let bad = [
        "SELECT id FROM customers JOIN orders ON customers.id = orders.customer_id",
        "SELECT foo FROM customers",
        "SELECT id FROM unknown",
        "SELECT id FROM customers WHERE foo = 2",
        "SELECT id FROM customers WHERE id = 1 OR id = 2",
        "SELECT id FROM customers ORDER BY id",
        "SELECT id FROM customers JOIN orders ON customers.id > orders.customer_id",
    ];
    for sql in bad {
        assert!(query(&db, sql, true).is_err(), "expected error for {sql}");
    }
}

#[test]
fn hash_join_rejects_null_keys() {
    let mut db = Database::default();
    db.insert(Table::from_csv("a", "key,name\n,missing\n1,match\n").unwrap());
    db.insert(Table::from_csv("b", "key,val\n,notjoined\n1,joined\n").unwrap());
    let sql = "SELECT a.name, b.val FROM a JOIN b ON a.key = b.key";
    let result = query(&db, sql, true).unwrap();
    assert_eq!(result.rows, vec![vec![
        Value::Text("match".into()),
        Value::Text("joined".into()),
    ]]);
}
