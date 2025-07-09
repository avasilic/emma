use std::io::Result;

fn main() -> Result<()> {
    let proto_file = "../../proto/climate_point.proto";
    let proto_dir = "../../proto";

    // Tell Cargo to rerun this build script if the proto file changes
    println!("cargo:rerun-if-changed={proto_file}");

    // Compile the protobuf - this generates files in OUT_DIR by default
    prost_build::compile_protos(&[proto_file], &[proto_dir])?;

    Ok(())
}
