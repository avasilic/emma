// This module contains the generated protobuf types

// Include generated protobuf code from OUT_DIR
pub mod proto_v1 {
    include!(concat!(env!("OUT_DIR"), "/proto.v1.rs"));
}

// Re-export for convenience
pub use proto_v1::DataPoint;
