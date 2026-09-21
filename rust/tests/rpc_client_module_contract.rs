#![forbid(unsafe_code)]

#[path = "../src/rpc_client_module.rs"]
mod rpc_client_module;

use rpc_client_module::{RpcClientModulePath, RpcPublicationOrigin};

#[test]
fn generated_rest_and_native_rpc_keep_same_semantic_import_path() {
    let generated = RpcClientModulePath::from_operation_key(
        "demo.users.get_user",
        RpcPublicationOrigin::RestGenerated,
    )
    .expect("generated REST RPC module");
    let native = RpcClientModulePath::from_operation_key(
        "demo.users.get_user",
        RpcPublicationOrigin::RpcNative,
    )
    .expect("native RPC module");

    assert_eq!(generated.import_path("/"), "users/get_user");
    assert_eq!(generated.import_path("/"), native.import_path("/"));
    assert_ne!(generated.origin, native.origin);
}

#[test]
fn subtree_selection_does_not_pull_unrelated_namespaces() {
    let get_user = RpcClientModulePath::from_operation_key(
        "demo.users.get_user",
        RpcPublicationOrigin::RestGenerated,
    )
    .unwrap();
    let create_invoice = RpcClientModulePath::from_operation_key(
        "demo.billing.create_invoice",
        RpcPublicationOrigin::RpcNative,
    )
    .unwrap();
    let users = vec!["users".to_owned()];

    assert!(get_user.belongs_to_subtree(&users));
    assert!(!create_invoice.belongs_to_subtree(&users));
}
