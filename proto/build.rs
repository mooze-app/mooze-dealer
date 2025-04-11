fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("src/proto/wallet.proto")?;
    tonic_build::compile_protos("src/proto/swap.proto")?;
    Ok(())
}
