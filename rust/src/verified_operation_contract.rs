//! Fail-closed construction of shared-operation RPC IR from authored route.rs.

use crate::{
    analyze_shared_operation_route_source, rpc_operation_contract_with_route_source,
    verify_shared_operation_invocations, RouteMap, RpcOperationContract, RpcOperationScope,
};

pub fn verified_rpc_operation_contract(
    map: &RouteMap,
    route_key: &str,
    scope: RpcOperationScope,
    repository: Option<&str>,
    commit_sha: Option<&str>,
    route_file: &str,
    route_source_text: &str,
) -> Result<RpcOperationContract, String> {
    let analysis = analyze_shared_operation_route_source(route_file, route_source_text)
        .map_err(|error| error.to_string())?;
    verify_shared_operation_invocations(route_file, route_source_text, &analysis)?;
    rpc_operation_contract_with_route_source(
        map,
        route_key,
        scope,
        repository,
        commit_sha,
        route_source_text,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> RouteMap {
        RouteMap::from_json_str(
            r#"{
              "schema_version":"1.0.0",
              "service":"fiducia-api-server",
              "map":{
                "find_user":{
                  "path":"/v1/users/{user_id}",
                  "methods":["GET"],
                  "rpc_key":"fiducia_cloud.users.find_user",
                  "binding":{
                    "annotation":"ores_operation",
                    "file":"src/routes/v1/users/[user_id]/route.rs"
                  }
                }
              }
            }"#,
        )
        .expect("map")
    }

    #[test]
    fn declared_pair_must_really_call_invoker() {
        let source = r#"
            #[ores_operation(key = "fiducia_cloud.users.find_user", scope = "regular")]
            async fn find_user(ctx: OperationContext, input: FindUserInput) -> Output { todo!() }

            #[ores_route(operation = find_user)]
            pub async fn get() -> HttpResult {
                unrelated().await
            }
        "#;
        let error = verified_rpc_operation_contract(
            &map(),
            "find_user",
            RpcOperationScope::Regular,
            None,
            None,
            "src/routes/v1/users/[user_id]/route.rs",
            source,
        )
        .expect_err("adapter bypass must fail");
        assert!(error.contains("exactly once"));
    }

    #[test]
    fn correctly_forwarded_pair_yields_shared_operation_ir() {
        let source = r#"
            #[ores_operation(key = "fiducia_cloud.users.find_user", scope = "regular")]
            async fn find_user(ctx: OperationContext, input: FindUserInput) -> Output { todo!() }

            #[ores_route(operation = find_user)]
            pub async fn get() -> HttpResult {
                __ores_invoke_find_user(ctx, input).await.into()
            }
        "#;
        let contract = verified_rpc_operation_contract(
            &map(),
            "find_user",
            RpcOperationScope::Regular,
            None,
            None,
            "src/routes/v1/users/[user_id]/route.rs",
            source,
        )
        .expect("verified contract");
        assert_eq!(contract.source.execution_model, "shared_operation");
        assert_eq!(contract.source.operation.as_deref(), Some("find_user"));
    }
}
