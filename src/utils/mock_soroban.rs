use anyhow::Result;

pub fn validate_wasm(bytes: &[u8]) -> Result<()> {
    // A minimal "wasm header" check to avoid treating arbitrary files as wasm.
    if bytes.len() < 8 {
        anyhow::bail!("Wasm file too small");
    }
    if &bytes[..4] != b"\0asm" {
        anyhow::bail!("Missing wasm header");
    }
    Ok(())
}

