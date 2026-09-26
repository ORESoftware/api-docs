namespace OresApiDocs.ModuleInterfaceCodegen;

alias ModuleInterfaceCodegenContractVersion = "ores.module-interface-codegen/v1";

enum ModuleInterfaceRuntimeProfile {
  ores_stack,
  scintilla,
  beam_scale,
}

enum ModuleInterfaceLanguage {
  rust,
  typescript,
  dart,
  erlang,
  gleam,
  sml,
  ocaml,
  racket,
  ada,
  modula2,
  modula3,
  haskell,
  wit,
}

model ModuleInterfaceSpec {
  module_name: string;
  operation_name: string;
  contract_id: string;
}

model ModuleInterfaceCodegenRequest {
  contract_version: ModuleInterfaceCodegenContractVersion;
  runtime_profile: ModuleInterfaceRuntimeProfile;
  languages: ModuleInterfaceLanguage[];
  module: ModuleInterfaceSpec;
}
