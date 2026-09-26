#![allow(clippy::needless_return)]

use ores_api_docs::{
    render_module_interface_matrix, ModuleInterfaceLanguage, ModuleInterfaceRuntimeProfile,
    ModuleInterfaceSpec,
};

fn language_ids(artifacts: &[ores_api_docs::GeneratedModuleInterface]) -> Vec<&'static str> {
    let ids = artifacts
        .iter()
        .map(|artifact| artifact.language.as_str())
        .collect::<Vec<_>>();

    return ids;
}

#[test]
fn ores_stack_profile_renders_full_contract_matrix_deterministically() {
    let spec = ModuleInterfaceSpec::new(
        "catalog_worker",
        "handle",
        "ores-stack.worker.v1",
    );

    let first = render_module_interface_matrix(
        ModuleInterfaceRuntimeProfile::OresStack,
        &ModuleInterfaceLanguage::ALL,
        &spec,
    )
    .expect("ORES Stack projection matrix should render");
    let second = render_module_interface_matrix(
        ModuleInterfaceRuntimeProfile::OresStack,
        &ModuleInterfaceLanguage::ALL,
        &spec,
    )
    .expect("second ORES Stack render should succeed");

    assert_eq!(first, second);
    assert_eq!(language_ids(&first).len(), 13);
    assert!(language_ids(&first).contains(&"dart"));
}

#[test]
fn scintilla_profile_renders_full_container_guest_matrix() {
    let spec = ModuleInterfaceSpec::new(
        "scintilla_function",
        "invoke",
        "scintilla.function.v1",
    );

    let artifacts = render_module_interface_matrix(
        ModuleInterfaceRuntimeProfile::Scintilla,
        &ModuleInterfaceLanguage::ALL,
        &spec,
    )
    .expect("Scintilla should admit the full language matrix");

    assert_eq!(
        language_ids(&artifacts),
        vec![
            "rust",
            "typescript",
            "dart",
            "erlang",
            "gleam",
            "sml",
            "ocaml",
            "racket",
            "ada",
            "modula2",
            "modula3",
            "haskell",
            "wit",
        ]
    );
}

#[test]
fn beamscale_profile_renders_only_direct_beam_guest_languages() {
    let spec = ModuleInterfaceSpec::new(
        "beam_worker",
        "handle",
        "bmscl.worker.v1",
    );
    let direct_guests = [
        ModuleInterfaceLanguage::Erlang,
        ModuleInterfaceLanguage::Gleam,
    ];

    let artifacts = render_module_interface_matrix(
        ModuleInterfaceRuntimeProfile::BeamScale,
        &direct_guests,
        &spec,
    )
    .expect("BeamScale BEAM guest matrix should render");

    assert_eq!(language_ids(&artifacts), vec!["erlang", "gleam"]);

    for language in ModuleInterfaceLanguage::ALL {
        if direct_guests.contains(&language) {
            continue;
        }

        let error = ores_api_docs::render_module_interface(
            ModuleInterfaceRuntimeProfile::BeamScale,
            language,
            &spec,
        )
        .expect_err("BeamScale must reject non-BEAM direct guest projections");

        assert_eq!(error.to_string().contains("does not admit"), true);
    }
}
