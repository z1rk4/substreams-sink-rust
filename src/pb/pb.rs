// @generated
pub mod common {
    // @@protoc_insertion_point(attribute:common.v1)
    pub mod v1 {
        include!("common.v1.rs");
        // @@protoc_insertion_point(common.v1)
    }
}
pub mod google {
    // @@protoc_insertion_point(attribute:google.protobuf)
    pub mod protobuf {
        include!("google.protobuf.rs");
        // @@protoc_insertion_point(google.protobuf)
    }
}
pub mod sf {
    pub mod antelope {
        pub mod r#type {
            // @@protoc_insertion_point(attribute:sf.antelope.type.v1)
            pub mod v1 {
                include!("sf.antelope.type.v1.rs");
                // @@protoc_insertion_point(sf.antelope.type.v1)
            }
        }
    }
    // @@protoc_insertion_point(attribute:sf.substreams)
    pub mod substreams {
        include!("sf.substreams.rs");
        // @@protoc_insertion_point(sf.substreams)
        pub mod rpc {
            // @@protoc_insertion_point(attribute:sf.substreams.rpc.v2)
            pub mod v2 {
                include!("sf.substreams.rpc.v2.rs");
                // @@protoc_insertion_point(sf.substreams.rpc.v2)
            }
        }
        pub mod sink {
            pub mod kv {
                // @@protoc_insertion_point(attribute:sf.substreams.sink.kv.v1)
                pub mod v1 {
                    include!("sf.substreams.sink.kv.v1.rs");
                    // @@protoc_insertion_point(sf.substreams.sink.kv.v1)
                }
            }
            pub mod service {
                // @@protoc_insertion_point(attribute:sf.substreams.sink.service.v1)
                pub mod v1 {
                    include!("sf.substreams.sink.service.v1.rs");
                    // @@protoc_insertion_point(sf.substreams.sink.service.v1)
                }
            }
            pub mod types {
                // @@protoc_insertion_point(attribute:sf.substreams.sink.types.v1)
                pub mod v1 {
                    include!("sf.substreams.sink.types.v1.rs");
                    // @@protoc_insertion_point(sf.substreams.sink.types.v1)
                }
            }
        }
        // @@protoc_insertion_point(attribute:sf.substreams.v1)
        pub mod v1 {
            include!("sf.substreams.v1.rs");
            // @@protoc_insertion_point(sf.substreams.v1)
            // @@protoc_insertion_point(attribute:sf.substreams.v1.test)
            pub mod test {
                include!("sf.substreams.v1.test.rs");
                // @@protoc_insertion_point(sf.substreams.v1.test)
            }
        }
    }
}
