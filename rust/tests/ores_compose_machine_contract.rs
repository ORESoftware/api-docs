use ores_api_docs::RouteMap;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const ORES_INTERFACES_SHA: &str = "9ff7362dca43455b84a5dc7eb56ee7d2347f8170";
const ORES_INTERFACES_URL: &str = "https://github.com/ORESoftware/ores-interfaces.git";

fn temp_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "api-docs-ores-compose-machine-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("temp root");
    root
}

fn git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .expect("git process");
    assert!(status.success(), "git {args:?} failed");
}

fn checkout_shared_contract(root: &Path) -> PathBuf {
    let source = root.join("ores-interfaces");
    fs::create_dir_all(&source).expect("source");
    git(&source, &["init"]);
    git(&source, &["remote", "add", "origin", ORES_INTERFACES_URL]);
    git(
        &source,
        &["fetch", "--depth=1", "origin", ORES_INTERFACES_SHA],
    );
    git(&source, &["checkout", "--detach", ORES_INTERFACES_SHA]);
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&source)
        .output()
        .expect("rev-parse");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("utf8").trim(),
        ORES_INTERFACES_SHA
    );
    source
}

fn schema_def<'a>(schema: &'a Value, name: &str) -> &'a Value {
    &schema["$defs"][name]
}

#[test]
fn ores_compose_machine_route_map_is_bound_to_exact_shared_contract() {
    let root = temp_root();
    let source = checkout_shared_contract(&root);
    let schema_path = source.join("contracts/ores-compose-machine/v1/authored.schema.json");
    let shared: Value = serde_json::from_str(&fs::read_to_string(schema_path).expect("schema"))
        .expect("shared schema json");

    let route_map_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/ores-compose-machine.route-map.json");
    let route_map_text = fs::read_to_string(route_map_path).expect("route map");
    let map = RouteMap::from_json_str(&route_map_text).expect("valid route map");

    let ensure = map.lookup("ensure").expect("ensure route");
    assert_eq!(ensure.path, "/v1/ensure");
    assert_eq!(ensure.methods, ["POST"]);
    assert_eq!(
        ensure.request_schema.as_ref().expect("request schema"),
        schema_def(&shared, "EnsureRequest")
    );
    assert_eq!(
        ensure.response_schema.as_ref().expect("response schema"),
        schema_def(&shared, "EnqueueResponse")
    );
    assert_eq!(
        ensure.error_schema.as_ref().expect("error schema"),
        schema_def(&shared, "MachineErrorResponse")
    );

    let request = ensure.request_schema.as_ref().expect("request schema");
    for field in ["project", "session", "service"] {
        assert_eq!(
            request["properties"][field]["pattern"],
            "^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$"
        );
    }
    for forbidden in ["command", "argv", "shell", "backend", "host", "port"] {
        assert!(
            request["properties"].get(forbidden).is_none(),
            "{forbidden}"
        );
    }

    let status = map.lookup("job_status").expect("job status");
    assert_eq!(status.path, "/v1/jobs/{job_id}");
    let path_params = status.path_params.as_ref().expect("path params");
    assert_eq!(
        path_params["properties"]["job_id"],
        schema_def(&shared, "JobStatusResponse")["properties"]["job_id"]
    );
    assert_eq!(
        status.response_schema.as_ref().expect("status response")["properties"]["state"],
        schema_def(&shared, "JobStatusResponse")["properties"]["state"]
    );

    let readiness = map.lookup("readiness").expect("readiness");
    assert_eq!(readiness.path, "/v1/readiness");
    assert_eq!(
        readiness
            .response_schema
            .as_ref()
            .expect("readiness response")["properties"]["ready"],
        schema_def(&shared, "ReadinessResponse")["properties"]["ready"]
    );

    let ingress_pattern = schema_def(&shared, "MachineIngress")["properties"]["authority"]
        ["pattern"]
        .as_str()
        .expect("ingress pattern");
    assert_eq!(ingress_pattern, "^(?:127\\.|\\[::1\\]:|/)");
    assert!(!route_map_text.contains("backend_url"));
    assert!(!route_map_text.contains("replica_address"));
    assert!(!route_map_text.contains("10.0.0."));

    let _ = fs::remove_dir_all(root);
}
