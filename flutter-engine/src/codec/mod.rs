pub mod json_codec;
pub mod standard_codec;

#[derive(Serialize, Deserialize, Debug)]
pub struct MethodCall<T> {
    pub method: String,
    pub args: T,
}

pub enum CodecTypes {
    JsonMessageCodec,
    StandardMessageCodec,
}

pub enum MethodCallResult<R> {
    Ok(R),
    Err { code: String, message: String, data: R }
}

pub trait MethodCodec {
    type R;

    fn decode_method_call(buf: &[u8]) -> Option<MethodCall<Self::R>>;
    fn decode_envelope(buf: &[u8]) -> Option<MethodCallResult<Self::R>>;
    fn encode_success_envelope(v: &Self::R) -> Vec<u8>;
    fn encode_error_envelope(code: &str, message: &str, v: &Self::R) -> Vec<u8>;
    fn encode_method_call(v: &MethodCall<Self::R>) -> Vec<u8>;
}