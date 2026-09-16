#![forbid(unsafe_code)]
#![deny(clippy::all)]

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use ores_api_docs::{rpc_pool_bindings, RouteMap};

fn usage() -> &'static str {
    "api-docs-pool\n\nUsage:\n  api-docs-pool --map PATH --dto-module RUST_PATH --out PATH [--check]\n"
}

fn require_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn absolute(path: PathBuf) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path)
    } else {
        env::current_dir()
            .map(|root| root.join(path))
            .map_err(|error| format!("current_dir: {error}"))
    }
}

fn write_or_check(path: &Path, generated: &str, check: bool) -> Result<(), String> {
    if check {
        let current = fs::read_to_string(path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        if current != generated {
            return Err(format!(
                "{} is stale; regenerate it with api-docs-pool",
                path.display()
            ));
        }
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    fs::write(path, generated).map_err(|error| format!("write {}: {error}", path.display()))
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        println!("{}", usage());
        return Ok(());
    }

    let mut map = None;
    let mut dto_module = None;
    let mut out = None;
    let mut check = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--map" => map = Some(PathBuf::from(require_value(&args, &mut index, "--map")?)),
            "--dto-module" => dto_module = Some(require_value(&args, &mut index, "--dto-module")?),
            "--out" => out = Some(PathBuf::from(require_value(&args, &mut index, "--out")?)),
            "--check" => check = true,
            flag => return Err(format!("unexpected argument: {flag}\n\n{}", usage())),
        }
        index += 1;
    }

    let map_path = absolute(map.ok_or_else(|| "--map is required".to_owned())?)?;
    let out_path = absolute(out.ok_or_else(|| "--out is required".to_owned())?)?;
    let dto_module = dto_module.ok_or_else(|| "--dto-module is required".to_owned())?;

    let source = fs::read_to_string(&map_path)
        .map_err(|error| format!("read {}: {error}", map_path.display()))?;
    let route_map = RouteMap::from_json_str(&source)
        .map_err(|error| format!("parse {}: {error}", map_path.display()))?;
    let generated = rpc_pool_bindings(&route_map, &dto_module)?;
    write_or_check(&out_path, &generated, check)
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
