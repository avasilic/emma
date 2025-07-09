// This module contains the generated protobuf types

// Include generated protobuf code from OUT_DIR
pub mod proto_v3 {
    include!(concat!(env!("OUT_DIR"), "/proto.v3.rs"));
}

// Re-export for convenience
pub use proto_v3::ClimatePoint;
