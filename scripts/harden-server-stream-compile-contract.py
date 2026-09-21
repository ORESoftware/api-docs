#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text()
    if new in text:
        return
    count = text.count(old)
    if count != 1:
        raise SystemExit(
            f"{path}: expected exactly one patch anchor, found {count}: {old[:100]!r}"
        )
    path.write_text(text.replace(old, new, 1))


# OperationSpec is the generated compile-time authority for stream shape.
operation_spec = ROOT / "rust/src/operation_spec.rs"
replace_once(
    operation_spec,
    "use crate::RpcPayloadCodec;",
    "use crate::{RpcPayloadCodec, RpcStreamMode};",
)
replace_once(
    operation_spec,
    "    const KEY: &'static str;\n"
    "    const CODECS: &'static [RpcPayloadCodec];\n"
    "    const DEFAULT_CODEC: RpcPayloadCodec;\n"
    "}",
    "    const KEY: &'static str;\n"
    "    const CODECS: &'static [RpcPayloadCodec];\n"
    "    const DEFAULT_CODEC: RpcPayloadCodec;\n"
    "    /// Compile-time stream shape for this generated semantic operation.\n"
    "    ///\n"
    "    /// Unary remains the compatibility default. Generators MUST emit this\n"
    "    /// constant explicitly for every non-unary operation so stale generated\n"
    "    /// specs fail against `#[ores_operation(stream = ...)]` during cargo check.\n"
    "    const STREAM: RpcStreamMode = RpcStreamMode::Unary;\n"
    "}",
)

# Keep proc-macro output on the public runtime ABI rather than private module
# paths, and expose the canonical authored alias to applications.
lib_rs = ROOT / "rust/src/lib.rs"
replace_once(
    lib_rs,
    "    OperationServerStream, RpcV1ServerStream,\n};",
    "    OperationServerStream, RpcV1ServerStream, ServerStreamResult,\n};",
)
replace_once(
    lib_rs,
    "pub use typed_operation_context::{invoke_typed_context_operation, TypedOperationContext};",
    "pub use typed_operation_context::{\n"
    "    invoke_typed_context_operation, invoke_typed_context_server_stream_operation,\n"
    "    TypedOperationContext,\n"
    "};",
)

macro_rs = ROOT / "macros/operation-rust/src/lib.rs"
text = macro_rs.read_text()

if "__ORES_STREAM_MODE_ASSERT_" not in text:
    old = '        let assert_name = format_ident!("__ores_assert_spec_{}", operation_name);\n'
    new = (
        old
        + "        let stream_assert_name = format_ident!(\n"
        + '            "__ORES_STREAM_MODE_ASSERT_{}",\n'
        + "            operation_name.to_string().to_ascii_uppercase()\n"
        + "        );\n"
    )
    if old not in text:
        raise SystemExit("macro: assert-name anchor missing")
    text = text.replace(old, new, 1)

    old = """                    // This non-generic where-clause is checked when the item is
                    // compiled. A handwritten unary handler therefore cannot
                    // return a body or error type different from the generated
                    // client/backend contract.
                    #[doc(hidden)]
                    fn #assert_name()
"""
    new = """                    #[doc(hidden)]
                    const #stream_assert_name: () = {
                        match <#spec_ty as ::ores_api_docs::OperationSpec>::STREAM {
                            ::ores_api_docs::RpcStreamMode::Unary => (),
                            _ => panic!(\"#[ores_operation(stream = \\\"unary\\\")] metadata stream mode disagrees with OperationSpec::STREAM\"),
                        }
                    };

                    // This non-generic where-clause is checked when the item is
                    // compiled. A handwritten unary handler therefore cannot
                    // return a body or error type different from the generated
                    // client/backend contract.
                    #[doc(hidden)]
                    fn #assert_name()
"""
    if old not in text:
        raise SystemExit("macro: unary assertion anchor missing")
    text = text.replace(old, new, 1)

    old = """                    > {
                        #assert_name();
                        ::ores_api_docs::typed_operation_context::invoke_typed_context_operation(
"""
    new = """                    > {
                        let _ = #stream_assert_name;
                        #assert_name();
                        ::ores_api_docs::typed_operation_context::invoke_typed_context_operation(
"""
    if old not in text:
        raise SystemExit("macro: unary invoke anchor missing")
    text = text.replace(old, new, 1)

    old = """            \"server_stream\" => Ok(quote! {
                #item

                #descriptor

                /// Generated shared server-stream boundary. The authored async
                /// handler must return exactly the operation-typed stream shape;
                /// the helper's Future bound makes metadata/type disagreement a
                /// Rust compile error rather than a generator/runtime trap.
                #[doc(hidden)]
                pub(crate) async fn #invoke_name(
                    #context_name: #context_ty,
                ) -> ::core::result::Result<
                    ::ores_api_docs::operation_server_stream::ServerStreamResult<#spec_ty>,
                    ::ores_api_docs::OperationInvokeError<
                        <#spec_ty as ::ores_api_docs::OperationSpec>::Error
                    >,
                > {
                    ::ores_api_docs::typed_operation_context::invoke_typed_context_server_stream_operation(
                        &#descriptor_name,
                        #context_name,
                        #operation_name,
                    )
                    .await
                }
            }),
"""
    new = """            \"server_stream\" => {
                let return_spec = server_stream_result_spec(&item.sig.output)?;
                if type_source(&return_spec) != type_source(spec_ty) {
                    return Err(syn::Error::new_spanned(
                        &item.sig.output,
                        \"#[ores_operation(stream = \\\"server_stream\\\")] requires return type ServerStreamResult<OperationSpec>\",
                    ));
                }
                Ok(quote! {
                    #item

                    #[doc(hidden)]
                    const #stream_assert_name: () = {
                        match <#spec_ty as ::ores_api_docs::OperationSpec>::STREAM {
                            ::ores_api_docs::RpcStreamMode::ServerStream => (),
                            _ => panic!(\"#[ores_operation(stream = \\\"server_stream\\\")] metadata stream mode disagrees with OperationSpec::STREAM\"),
                        }
                    };

                    #descriptor

                    /// Generated shared server-stream boundary. The authored async
                    /// handler must return exactly the operation-typed stream shape;
                    /// the helper's Future bound makes metadata/type disagreement a
                    /// Rust compile error rather than a generator/runtime trap.
                    #[doc(hidden)]
                    pub(crate) async fn #invoke_name(
                        #context_name: #context_ty,
                    ) -> ::core::result::Result<
                        ::ores_api_docs::ServerStreamResult<#spec_ty>,
                        ::ores_api_docs::OperationInvokeError<
                            <#spec_ty as ::ores_api_docs::OperationSpec>::Error
                        >,
                    > {
                        let _ = #stream_assert_name;
                        ::ores_api_docs::invoke_typed_context_server_stream_operation(
                            &#descriptor_name,
                            #context_name,
                            #operation_name,
                        )
                        .await
                    }
                })
            }
"""
    if old not in text:
        raise SystemExit("macro: current server-stream branch anchor missing")
    text = text.replace(old, new, 1)

    old = """    let is_streaming = stream != \"unary\";
    if has_stream_suffix && !is_streaming {
"""
    new = """    let is_streaming = stream != \"unary\";
    if item.sig.inputs.len() == 2 && is_streaming {
        return Err(syn::Error::new_spanned(
            item,
            \"streaming ores_operation handlers require canonical one-argument TypedOperationContext<State, OperationSpec>\",
        ));
    }
    if has_stream_suffix && !is_streaming {
"""
    if old not in text:
        raise SystemExit("macro: migration stream anchor missing")
    text = text.replace(old, new, 1)

    text = text.replace(
        "                reject_unary_result_for_server_stream(&item.sig.output)?;",
        "                server_stream_result_spec(&item.sig.output)?;",
        1,
    )

    start = text.index("fn reject_unary_result_for_server_stream(output: &ReturnType)")
    end = text.index("\nfn typed_context_spec(ty: &Type)", start)
    helper = r'''fn server_stream_result_spec(output: &ReturnType) -> syn::Result<Type> {
    let ReturnType::Type(_, ty) = output else {
        return Err(syn::Error::new_spanned(
            output,
            "#[ores_operation(stream = \"server_stream\")] requires return type ServerStreamResult<OperationSpec>",
        ));
    };
    let Type::Path(path) = ty.as_ref() else {
        return Err(syn::Error::new_spanned(
            ty,
            "#[ores_operation(stream = \"server_stream\")] requires return type ServerStreamResult<OperationSpec>",
        ));
    };
    let Some(segment) = path.path.segments.last() else {
        return Err(syn::Error::new_spanned(ty, "missing server stream return type"));
    };
    if segment.ident != "ServerStreamResult" {
        return Err(syn::Error::new_spanned(
            ty,
            "#[ores_operation(stream = \"server_stream\")] requires return type ServerStreamResult<OperationSpec>",
        ));
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return Err(syn::Error::new_spanned(
            &segment.arguments,
            "ServerStreamResult must declare exactly one OperationSpec type",
        ));
    };
    let types = arguments
        .args
        .iter()
        .filter_map(|argument| match argument {
            GenericArgument::Type(ty) => Some(ty.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if types.len() != 1 {
        return Err(syn::Error::new_spanned(
            arguments,
            "ServerStreamResult must declare exactly one OperationSpec type",
        ));
    }
    Ok(types[0].clone())
}
'''
    text = text[:start] + helper + text[end:]

    # Keep unit tests on the canonical typed path now that migration streaming
    # is deliberately rejected.
    text = text.replace(
        '        let item = operation("watch_users_stream");\n        let parsed = validate_operation(\n            &args(\n                r#"key = "demo.users.watch_users_stream", stream = "server_stream""#,\n            ),',
        '        let item = canonical_operation(\n            "watch_users_stream",\n            "ServerStreamResult<WatchEvents>",\n        );\n        let parsed = validate_operation(\n            &args(\n                r#"spec = WatchEvents, key = "demo.users.watch_users_stream", stream = "server_stream""#,\n            ),',
        1,
    )
    text = text.replace(
        '        assert!(error.to_string().contains("cannot return unary Result"));',
        '        assert!(error\n            .to_string()\n            .contains("requires return type ServerStreamResult<OperationSpec>"));',
        1,
    )
    text = text.replace(
        '        let item = operation("watch_users");\n        let error = validate_operation(\n            &args(r#"key = "demo.users.watch_users", stream = "server_stream""#),',
        '        let item = canonical_operation("watch_users", "ServerStreamResult<WatchEvents>");\n        let error = validate_operation(\n            &args(r#"spec = WatchEvents, key = "demo.users.watch_users", stream = "server_stream""#),',
        1,
    )

    macro_rs.write_text(text)

# Make the three-way link normative in the ABI docs.
contract = ROOT / "docs/server-stream-lambda-abi-contract.md"
contract_text = contract.read_text()
contract_text = contract_text.replace(
    "4. The authored Rust function returns `OperationServerStream<ResponseBody, Error>` so the compiler ties each item/error to the same operation contract that generates clients.",
    "4. The authored Rust function returns `ServerStreamResult<OperationSpec>`; `OperationSpec::STREAM`, `#[ores_operation(stream = ...)]`, and that return shape are compile-time-linked and must agree.",
)
contract.write_text(contract_text)

abi = ROOT / "docs/server-stream-lambda-abi.md"
abi_text = abi.read_text()
abi_text = abi_text.replace(
    "`server_stream` operations keep the same contract item type (`OperationSpec::ResponseBody`) while authored Rust handlers return `OperationServerStream<ResponseBody, Error>`.",
    "`server_stream` operations keep the same contract item type (`OperationSpec::ResponseBody`) while authored Rust handlers return `ServerStreamResult<OperationSpec>`. `OperationSpec::STREAM`, macro stream metadata, and the Rust return shape are compile-time-linked so drift fails during `cargo check`.",
)
abi.write_text(abi_text)
