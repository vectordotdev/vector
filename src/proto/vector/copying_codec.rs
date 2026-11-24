use std::marker::PhantomData;

use bytes::Buf;
use prost::Message;
use tonic::{
    Status,
    codec::{BufferSettings, Codec, DecodeBuf, Decoder},
};
use tonic_prost::ProstCodec;

#[derive(Debug, Default)]
pub(super) struct CopyingProstCodec<T, U>(PhantomData<(T, U)>);

impl<T, U> Codec for CopyingProstCodec<T, U>
where
    T: Message + Send + 'static,
    U: Message + Default + Send + 'static,
{
    type Encode = T;
    type Decode = U;
    type Encoder = <ProstCodec<T, U> as Codec>::Encoder;
    type Decoder = CopyingProstDecoder<U>;

    fn encoder(&mut self) -> Self::Encoder {
        ProstCodec::<T, U>::raw_encoder(BufferSettings::default())
    }

    fn decoder(&mut self) -> Self::Decoder {
        CopyingProstDecoder(PhantomData)
    }
}

pub(super) struct CopyingProstDecoder<U>(PhantomData<U>);

impl<U: Message + Default> Decoder for CopyingProstDecoder<U> {
    type Item = U;
    type Error = Status;

    fn decode(&mut self, buf: &mut DecodeBuf<'_>) -> Result<Option<U>, Status> {
        U::decode(CopyingBuf(buf))
            .map(Some)
            .map_err(|error| Status::internal(error.to_string()))
    }
}

struct CopyingBuf<B>(B);

impl<B: Buf> Buf for CopyingBuf<B> {
    fn remaining(&self) -> usize {
        self.0.remaining()
    }

    fn chunk(&self) -> &[u8] {
        self.0.chunk()
    }

    fn advance(&mut self, count: usize) {
        self.0.advance(count);
    }

}
