use std::error::Error;
use vergen::EmitBuilder;

fn main() -> Result<(), Box<dyn Error>> {
    EmitBuilder::builder()
        .build_timestamp()
        .git_sha(true)
        .emit()?;
    Ok(())
}
