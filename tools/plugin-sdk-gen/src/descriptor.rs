#[derive(Clone, PartialEq, prost::Message)]
pub struct FileDescriptorSet {
    #[prost(message, repeated, tag = "1")]
    pub file: Vec<FileDescriptorProto>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct FileDescriptorProto {
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub package: Option<String>,
    #[prost(message, repeated, tag = "4")]
    pub message_type: Vec<DescriptorProto>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct DescriptorProto {
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
    #[prost(message, repeated, tag = "2")]
    pub field: Vec<FieldDescriptorProto>,
    #[prost(message, repeated, tag = "8")]
    pub oneof_decl: Vec<OneofDescriptorProto>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct OneofDescriptorProto {
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct FieldDescriptorProto {
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
    #[prost(int32, optional, tag = "3")]
    pub number: Option<i32>,
    #[prost(int32, optional, tag = "5")]
    pub r#type: Option<i32>,
    #[prost(string, optional, tag = "6")]
    pub type_name: Option<String>,
    #[prost(message, optional, tag = "8")]
    pub options: Option<FieldOptions>,
    #[prost(int32, optional, tag = "9")]
    pub oneof_index: Option<i32>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct FieldOptions {
    #[prost(string, optional, tag = "50001")]
    pub path: Option<String>,
}
