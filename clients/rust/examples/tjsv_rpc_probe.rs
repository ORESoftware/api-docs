//! Fixed stdin/stdout conformance adapter, not a production CLI or validator.
use ores_api_docs_client::{decode_rpc_v1_call, decode_rpc_v1_receipt};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::error::Error;
use std::io::{self, Read, Write};

const LIMIT: u64 = 16 * 1024 * 1024;

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, Box<dyn Error>> {
    value.get(field).and_then(Value::as_str).ok_or_else(|| "invalid probe field".into())
}

fn run() -> Result<(), Box<dyn Error>> {
    if std::env::args_os().len() != 1 {
        return Err("probe accepts no arguments".into());
    }
    let mut input = Vec::new();
    io::stdin().lock().take(LIMIT + 1).read_to_end(&mut input)?;
    if input.len() as u64 > LIMIT {
        return Err("probe input exceeds limit".into());
    }
    let document: Value = serde_json::from_slice(&input)?;
    let fields = document.as_object().ok_or("probe must be an object")?;
    if fields.len() != 2 || text(&document, "schema")? != "ores.api-docs.rpc-probe/v1" {
        return Err("invalid probe schema or fields".into());
    }
    let cases = document.get("cases").and_then(Value::as_array).ok_or("missing probe cases")?;
    if cases.is_empty() || cases.len() > 4096 {
        return Err("invalid probe case count".into());
    }
    let mut names = HashSet::new();
    let mut results = Vec::with_capacity(cases.len());
    for case in cases {
        if case.as_object().ok_or("case must be an object")?.len() != 3 {
            return Err("unexpected probe case fields".into());
        }
        let name = text(case, "name")?;
        let kind = text(case, "kind")?;
        let encoded = text(case, "encoded")?;
        if name.is_empty() || !names.insert(name) {
            return Err("empty or duplicate probe name".into());
        }
        // Only the real decoder's Result::Err is rejection. Encoder and IO errors
        // propagate as process failure, and a panic is never recovered as evidence.
        let encoded_result = match kind {
            "call" => match decode_rpc_v1_call(encoded.as_bytes()) {
                Ok(value) => Some(value.encode()?),
                Err(_) => None,
            },
            "receipt" => match decode_rpc_v1_receipt(encoded.as_bytes()) {
                Ok(value) => Some(value.encode()?),
                Err(_) => None,
            },
            _ => return Err("unknown probe kind".into()),
        };
        results.push(match encoded_result {
            Some(bytes) => json!({"name": name, "kind": kind, "accepted": true, "encoded": String::from_utf8(bytes)?}),
            None => json!({"name": name, "kind": kind, "accepted": false}),
        });
    }
    let output = json!({"schema": "ores.api-docs.rpc-probe-result/v1", "runtime": "rust", "results": results});
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &output)?;
    writeln!(stdout)?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("probe execution failed: {error}");
        std::process::exit(3);
    }
}
