#!/usr/bin/env python3
"""Generate deterministic synthetic tables for MiniQuery benchmark experiments."""
import argparse
import csv
import random
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--customers", type=int, default=10_000)
    parser.add_argument("--orders", type=int, default=100_000)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--output-dir", type=Path, default=Path("benchdata"))
    args = parser.parse_args()
    if args.customers < 1 or args.orders < 1:
        parser.error("--customers and --orders must be positive")

    rng = random.Random(args.seed)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    customers_file = args.output_dir / "customers.csv"
    with customers_file.open("w", newline="", encoding="utf-8") as file:
        writer = csv.writer(file)
        writer.writerow(["id", "name", "region"])
        for customer_id in range(1, args.customers + 1):
            writer.writerow([customer_id, f"Customer{customer_id}", rng.choice(["US", "US", "CA", "MX", "GB"])])

    orders_file = args.output_dir / "orders.csv"
    with orders_file.open("w", newline="", encoding="utf-8") as file:
        writer = csv.writer(file)
        writer.writerow(["id", "customer_id", "amount", "status"])
        for order_id in range(1, args.orders + 1):
            writer.writerow([order_id, rng.randint(1, args.customers), rng.randint(1, 1000), rng.choice(["pending", "shipped"])])

    print(f"Generated {args.customers:,} customers and {args.orders:,} orders in {args.output_dir}")


if __name__ == "__main__":
    main()
