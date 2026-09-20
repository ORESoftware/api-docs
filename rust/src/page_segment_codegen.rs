use crate::project::sha256_hex;

/// Stable exported symbol used by standalone routing and generated web Lambda
/// wrappers to reach the nearest build-time-resolved `loading.rs` fallback for
/// one page. The function returns `None` when no ancestor segment owns one.
pub fn page_loading_entry_ident(source: &str) -> String {
    let mut out = String::from("__ores_loading_");
    for ch in source.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    out.push('_');
    out.push_str(&sha256_hex(source.as_bytes())[..16]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_symbol_is_stable_and_path_injective() {
        let a = page_loading_entry_ident("src/pages/a-b/page.rs");
        let b = page_loading_entry_ident("src/pages/a_b/page.rs");
        assert_ne!(a, b);
        assert_eq!(a, page_loading_entry_ident("src/pages/a-b/page.rs"));
    }
}