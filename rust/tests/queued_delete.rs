use ores_api_docs::RouteMap;
use serde_json::{json, Value};

fn queued_delete() -> Value {
    json!({
        "schema_version": "1.0.0",
        "service": "queued-delete-regression",
        "map": {
            "delete_item": {
                "path": "/items",
                "methods": ["DELETE"],
                "delivery": "opto_sync_queued",
                "opto_sync": {"table": "items", "operation": "delete"}
            }
        }
    })
}

#[test]
fn queued_delete_accepts_an_absent_request_body() {
    let map = RouteMap::from_value(queued_delete()).unwrap();
    assert!(map.lookup("delete_item").unwrap().request_schema.is_none());
}

#[test]
fn queued_delete_rejects_even_an_unconstrained_request_body() {
    let mut value = queued_delete();
    value["map"]["delete_item"]["request_schema"] = json!({});
    let error = RouteMap::from_value(value).unwrap_err();
    assert!(error.to_string().contains("a queued delete must not carry a request body"));
}
