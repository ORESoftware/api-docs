#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text()
    if new in text:
        return
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one patch anchor, found {count}: {old[:80]!r}")
    path.write_text(text.replace(old, new, 1))


# OperationSpec carries stream shape as compile-time authority. Unary is the
# compatibility default; generated/non-unary specs must opt in explicitly.
operation_spec = ROOT / "rust/src/operation_spec.rs"
replace_once(
    operation_spec,
    "use crate::RpcPayloadCodec;",
    "use crate::{RpcPayloadCodec, RpcStreamMode};",
)
replace_once(
    operation_spec,
    "    const KEY: &'static str;\n    const CODECS: &'static [RpcPayloadCodec];\n    const DEFAULT_CODEC: RpcPayloadCodec;\n}",
    "    const KEY: &'static str;\n    const CODECS: &'static [RpcPayloadCodec];\n    const DEFAULT_CODEC: RpcPayloadCodec;\n    /// Compile-time stream shape for this generated semantic operation.\n    ///\n    /// Unary remains the compatibility default. Generators MUST emit this\n    /// constant explicitly for every non-unary operation so stale generated\n    /// specs fail against `#[ores_operation(stream = ...)]` during cargo check.\n    const STREAM: RpcStreamMode = RpcStreamMode::Unary;\n}",
)

# Re-export the canonical authored alias and the streaming policy/invocation
# helper so proc-macro output depends only on the public runtime ABI.
lib_rs = ROOT / "rust/src/lib.rs"
replace_once(
    lib_rs,
    "    OperationServerStream, RpcV1ServerStream,\n};",
    "    OperationServerStream, RpcV1ServerStream, ServerStreamResult,\n};",
)
replace_once(
    lib_rs,
    "pub use typed_operation_context::{invoke_typed_context_operation, TypedOperationContext};",
    "pub use typed_operation_context::{\n    invoke_typed_context_operation, invoke_typed_context_server_stream_operation,\n    TypedOperationContext,\n};",
)

macro_rs = ROOT / "macros/operation-rust/src/lib.rs"
text = macro_rs.read_text()

# Idempotence: if the final helper exists, only validate the other required
# anchors are present and leave the file untouched.
if "fn server_stream_result_spec(output: &ReturnType)" not in text:
    old = "        let context_name = context_pat.ident.clone();\n        let context_ty = context.ty.as_ref();\n        let (success_ty, failure_ty) = result_types(&item.sig.output)?;\n\n        let descriptor_name = format_ident!("
    new = "        let context_name = context_pat.ident.clone();\n        let context_ty = context.ty.as_ref();\n\n        let descriptor_name = format_ident!("
    if old not in text:
        raise SystemExit("macro: canonical result-types anchor missing")
    text = text.replace(old, new, 1)

    old = '        let assert_name = format_ident!("__ores_assert_spec_{}", operation_name);\n'
    new = (
        old
        + '        let stream_assert_name = format_ident!(\n'
        + '            "__ORES_STREAM_MODE_ASSERT_{}",\n'
        + '            operation_name.to_string().to_ascii_uppercase()\n'
        + '        );\n'
    )
    if old not in text:
        raise SystemExit("macro: assert-name anchor missing")
    text = text.replace(old, new, 1)

    block_start = text.index("        return Ok(quote! {\n            #item")
    block_end_marker = "        });\n    }\n\n    // Migration-only compatibility"
    block_end = text.index(block_end_marker, block_start)
    replacement = r'''        let expanded = match meta.stream.as_str() {
            "unary" => {
                let (success_ty, failure_ty) = result_types(&item.sig.output)?;
                quote! {
                    #item

                    // The generated spec, macro metadata, and Rust return type
                    // are one compile-time contract. A stale generated stream
                    // mode therefore fails before generator/runtime dispatch.
                    #[doc(hidden)]
                    const #stream_assert_name: () = {
                        match <#spec_ty as ::ores_api_docs::OperationSpec>::STREAM {
                            ::ores_api_docs::RpcStreamMode::Unary => (),
                            _ => panic!("#[ores_operation(stream = \"unary\")] metadata stream mode disagrees with OperationSpec::STREAM"),
                        }
                    };

                    #[doc(hidden)]
                    fn #assert_name()
                    where
                        #spec_ty: ::ores_api_docs::OperationSpec<
                            ResponseBody = #success_ty,
                            Error = #failure_ty,
                        >,
                    {
                    }

                    #[doc(hidden)]
                    static #descriptor_name: ::ores_api_docs::OperationDescriptor =
                        ::ores_api_docs::OperationDescriptor {
                            key: #key,
                            codecs: &[#(#codecs),*],
                            default_codec: #default_codec,
                            audiences: &[#(#audiences),*],
                            scope: #scope,
                            stream: #stream_mode,
                        };

                    #[doc(hidden)]
                    pub(crate) async fn #invoke_name(
                        #context_name: #context_ty,
                    ) -> ::core::result::Result<
                        <#spec_ty as ::ores_api_docs::OperationSpec>::ResponseBody,
                        ::ores_api_docs::OperationInvokeError<
                            <#spec_ty as ::ores_api_docs::OperationSpec>::Error
                        >,
                    > {
                        let _ = #stream_assert_name;
                        #assert_name();
                        ::ores_api_docs::invoke_typed_context_operation(
                            &#descriptor_name,
                            #context_name,
                            #operation_name,
                        )
                        .await
                    }
                }
            }
            "server_stream" => {
                let return_spec = server_stream_result_spec(&item.sig.output)?;
                if type_source(&return_spec) != type_source(spec_ty) {
                    return Err(syn::Error::new_spanned(
                        &item.sig.output,
                        format!(
                            "#[ores_operation(stream = \"server_stream\")] requires return type ServerStreamResult<{}>",
                            type_source(spec_ty)
                        ),
                    ));
                }
                quote! {
                    #item

                    #[doc(hidden)]
                    const #stream_assert_name: () = {
                        match <#spec_ty as ::ores_api_docs::OperationSpec>::STREAM {
                            ::ores_api_docs::RpcStreamMode::ServerStream => (),
                            _ => panic!("#[ores_operation(stream = \"server_stream\")] metadata stream mode disagrees with OperationSpec::STREAM"),
                        }
                    };

                    #[doc(hidden)]
                    static #descriptor_name: ::ores_api_docs::OperationDescriptor =
                        ::ores_api_docs::OperationDescriptor {
                            key: #key,
                            codecs: &[#(#codecs),*],
                            default_codec: #default_codec,
                            audiences: &[#(#audiences),*],
                            scope: #scope,
                            stream: #stream_mode,
                        };

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
                }
            }
            "client_stream" | "bidi" => {
                return Err(syn::Error::new_spanned(
                    &item.sig.output,
                    format!(
                        "#[ores_operation(stream = \"{}\")] is not implemented by the canonical Rust operation runtime yet",
                        meta.stream
                    ),
                ));
            }
            _ => unreachable!("stream mode validated before expansion"),
        };
        return Ok(expanded);
'''
    text = text[:block_start] + replacement + text[block_end + len("        });\n") :]

    # Migration compatibility cannot provide the typed streaming guarantee.
    anchor = '''    let is_streaming = stream != "unary";
    if has_stream_suffix && !is_streaming {'''
    patched = '''    let is_streaming = stream != "unary";
    if item.sig.inputs.len() == 2 && is_streaming {
        return Err(syn::Error::new_spanned(
            item,
            "streaming ores_operation handlers require canonical one-argument TypedOperationContext<State, OperationSpec>",
        ));
    }
    if has_stream_suffix && !is_streaming {'''
    if anchor not in text:
        raise SystemExit("macro: migration stream anchor missing")
    text = text.replace(anchor, patched, 1)

    helper_anchor = "fn result_types(output: &ReturnType) -> syn::Result<(Type, Type)> {"
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
    if helper_anchor not in text:
        raise SystemExit("macro: result_types helper anchor missing")
    text = text.replace(helper_anchor, helper + helper_anchor, 1)
    macro_rs.write_text(text)

# Tighten the ABI docs to make the alias and three-way compile-time link
# normative rather than merely recommended.
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
