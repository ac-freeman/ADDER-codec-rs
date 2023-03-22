use crate::codec::compressed::adu::frame::Adu;
use crate::codec::compressed::adu::intrablock::{compress_d_residuals, decompress_d_residuals};
use crate::codec::compressed::adu::{AduComponentCompression, AduCompression};
use crate::codec::compressed::blocks::prediction::D_RESIDUALS_EMPTY;
use crate::codec::compressed::blocks::{DResidual, BLOCK_SIZE_AREA, D_ENCODE_NO_EVENT};
use crate::codec::compressed::stream::{CompressedInput, CompressedOutput};
use crate::codec::{CodecError, ReadCompression, WriteCompression};
use crate::codec_old::compressed::compression::{
    dt_resid_offset_i16, dt_resid_offset_i16_inverse, Contexts, DeltaTResidualSmall,
};
use crate::codec_old::compressed::fenwick::context_switching::FenwickModel;
use crate::DeltaT;
use arithmetic_coding::{Decoder, Encoder};
use bitstream_io::{BigEndian, BitRead, BitReader, BitWrite, BitWriter};
use std::io::{Cursor, Error, Read, Write};

#[derive(Debug, Clone, PartialEq)]
pub struct AduInterBlock {
    /// How many bits the dt_residuals are shifted by.
    pub(crate) shift_loss_param: u8,

    /// Prediction residuals of D between each event and the event in the previous block.
    pub(crate) d_residuals: [DResidual; BLOCK_SIZE_AREA],

    /// Prediction residuals of delta_t between each event and the event in the previous block.
    pub(crate) t_residuals: [i16; BLOCK_SIZE_AREA],
}

impl AduComponentCompression for AduInterBlock {
    fn compress(
        &self,
        encoder: &mut Encoder<FenwickModel, BitWriter<Vec<u8>, BigEndian>>,
        contexts: &mut Contexts,
        stream: &mut BitWriter<Vec<u8>, BigEndian>,
        dtm: DeltaT,
    ) -> Result<(), CodecError> {
        // Get the context references
        let mut d_context = contexts.d_context;
        let mut dt_context = contexts.dt_context;
        let mut u8_context = contexts.u8_general_context;

        encoder.model.set_context(u8_context);

        // Write the shift loss parameter.
        encoder.encode(Some(&(self.shift_loss_param as usize)), stream)?;

        // Write the d_residuals
        compress_d_residuals(&self.d_residuals, encoder, d_context, stream);

        // Write the dt_residuals
        compress_dt_residuals(
            &self.t_residuals,
            &self.d_residuals,
            encoder,
            dt_context,
            stream,
            dtm,
        );

        Ok(())
    }

    fn decompress(
        decoder: &mut Decoder<FenwickModel, BitReader<Cursor<Vec<u8>>, BigEndian>>,
        contexts: &mut Contexts,
        stream: &mut BitReader<Cursor<Vec<u8>>, BigEndian>,
        dtm: DeltaT,
    ) -> Self {
        // Initialize empty inter block
        let mut inter_block = Self {
            shift_loss_param: 0,
            d_residuals: D_RESIDUALS_EMPTY,
            t_residuals: [0; BLOCK_SIZE_AREA],
        };

        decoder.model.set_context(contexts.u8_general_context);

        // Read the shift loss parameter.
        inter_block.shift_loss_param = decoder.decode(stream).unwrap().unwrap() as u8;

        // Read the d_residuals
        decompress_d_residuals(
            &mut inter_block.d_residuals,
            decoder,
            contexts.d_context,
            stream,
        );

        // Read the dt_residuals
        decompress_dt_residuals(
            &mut inter_block.t_residuals,
            &inter_block.d_residuals,
            decoder,
            contexts.dt_context,
            stream,
            dtm,
        );

        inter_block
    }
}

fn compress_dt_residuals<W: Write>(
    dt_residuals: &[DeltaTResidualSmall; BLOCK_SIZE_AREA],
    d_residuals: &[DResidual; BLOCK_SIZE_AREA],
    encoder: &mut Encoder<FenwickModel, BitWriter<W, BigEndian>>,
    dt_context: usize,
    stream: &mut BitWriter<W, BigEndian>,
    delta_t_max: DeltaT,
) -> Result<(), CodecError> {
    encoder.model.set_context(dt_context);
    for (dt_residual, d_residual) in dt_residuals.iter().zip(d_residuals.iter()) {
        if *d_residual != D_ENCODE_NO_EVENT {
            encoder.encode(
                Some(&dt_resid_offset_i16(*dt_residual, delta_t_max)),
                stream,
            )?;
        }
    }
    Ok(())
}

fn decompress_dt_residuals<R: Read>(
    dt_residuals: &mut [DeltaTResidualSmall; BLOCK_SIZE_AREA],
    d_residuals: &[DResidual; BLOCK_SIZE_AREA],
    decoder: &mut Decoder<FenwickModel, BitReader<R, BigEndian>>,
    dt_context: usize,
    stream: &mut BitReader<R, BigEndian>,
    delta_t_max: DeltaT,
) {
    decoder.model.set_context(dt_context);
    for (dt_residual, d_residual) in dt_residuals.iter_mut().zip(d_residuals.iter()) {
        if *d_residual != D_ENCODE_NO_EVENT {
            let symbol = decoder.decode(stream).unwrap();
            *dt_residual = dt_resid_offset_i16_inverse(symbol.unwrap(), delta_t_max);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::codec::compressed::adu::interblock::AduInterBlock;
    use crate::codec::compressed::adu::intrablock::gen_random_intra_block;
    use crate::codec::compressed::adu::{add_eof, AduComponentCompression, AduCompression};
    use crate::codec::compressed::blocks::{BLOCK_SIZE_AREA, D_ENCODE_NO_EVENT};
    use crate::codec::compressed::stream::{CompressedInput, CompressedOutput};
    use crate::codec::encoder::Encoder;
    use crate::codec::{CodecMetadata, WriteCompression};
    use std::error::Error;
    use std::io::{BufReader, Cursor};

    fn setup_encoder() -> crate::codec::compressed::stream::CompressedOutput<Vec<u8>> {
        let meta = CodecMetadata {
            delta_t_max: 100,
            ref_interval: 100,
            ..Default::default()
        };
        // By building the CompressedOutput directly (rather than calling Encoder::new_compressed),
        // we can avoid writing the header and stuff for testing purposes.
        crate::codec::compressed::stream::CompressedOutput::new(meta, Vec::new())
    }

    fn compress_inter_block() -> Result<(AduInterBlock, Vec<u8>), Box<dyn Error>> {
        let mut encoder = setup_encoder();
        let intra_block = gen_random_intra_block(1234, encoder.meta.delta_t_max, Some(7));
        // For convenience, we'll just use the intra block's generator.
        let inter_block = AduInterBlock {
            shift_loss_param: intra_block.shift_loss_param,
            d_residuals: intra_block.d_residuals,
            t_residuals: intra_block.dt_residuals,
        };

        assert!(inter_block
            .compress(
                encoder.arithmetic_coder.as_mut().unwrap(),
                encoder.contexts.as_mut().unwrap(),
                encoder.stream.as_mut().unwrap(),
                encoder.meta.delta_t_max
            )
            .is_ok());

        add_eof(&mut encoder);

        let written_data = encoder.into_writer().unwrap();

        Ok((inter_block, written_data))
    }

    #[test]
    fn test_compress_inter_block() {
        let (_, written_data) = compress_inter_block().unwrap();
        let output_len = written_data.len();
        let input_len = 1028; // Rough approximation
        assert!(output_len < input_len);
        eprintln!("Written data: {:?}", written_data);
    }

    #[test]
    fn test_decompress_inter_block() {
        let (inter_block, written_data) = compress_inter_block().unwrap();
        let tmp_len = written_data.len();

        let mut bufreader = Cursor::new(written_data);
        let mut bitreader = bitstream_io::BitReader::endian(bufreader, bitstream_io::BigEndian);

        let mut compressed_input: CompressedInput<Cursor<Vec<u8>>> = CompressedInput::new(100, 100);
        let mut decoder = compressed_input.arithmetic_coder.as_mut().unwrap();
        let mut contexts = compressed_input.contexts.as_mut().unwrap();

        let decoded_inter_block =
            AduInterBlock::decompress(&mut decoder, &mut contexts, &mut bitreader, 100);

        decoder.model.set_context(contexts.eof_context);
        let eof = decoder.decode(&mut bitreader).unwrap();
        assert!(eof.is_none());
        assert_eq!(
            inter_block.shift_loss_param,
            decoded_inter_block.shift_loss_param
        );
        assert_eq!(inter_block.d_residuals, decoded_inter_block.d_residuals);
        assert_eq!(inter_block.t_residuals, decoded_inter_block.t_residuals);
    }

    #[test]
    fn test_decompress_mostly_empty_interblock() {
        let mut encoder = CompressedOutput::new(
            CodecMetadata {
                codec_version: 0,
                header_size: 0,
                time_mode: Default::default(),
                plane: Default::default(),
                tps: 0,
                ref_interval: 255,
                delta_t_max: 102000,
                event_size: 0,
                source_camera: Default::default(),
            },
            Vec::new(),
        );

        let dtm = encoder.meta.delta_t_max;
        let ref_interval = encoder.meta.ref_interval;
        // let mut encoder: Encoder<Vec<u8>> = Encoder::new_compressed(compression);

        let mut d_residuals = [D_ENCODE_NO_EVENT; BLOCK_SIZE_AREA];
        d_residuals[39] = -2;

        let mut t_residuals = [0; BLOCK_SIZE_AREA];
        t_residuals[39] = -1;
        let reference_block = AduInterBlock {
            shift_loss_param: 7,
            d_residuals,
            t_residuals,
        };

        assert!(reference_block
            .compress(
                encoder.arithmetic_coder.as_mut().unwrap(),
                encoder.contexts.as_mut().unwrap(),
                encoder.stream.as_mut().unwrap(),
                encoder.meta.delta_t_max
            )
            .is_ok());

        add_eof(&mut encoder);

        let written_data = encoder.into_writer().unwrap();

        let mut bufreader = Cursor::new(written_data);
        let mut bitreader = bitstream_io::BitReader::endian(bufreader, bitstream_io::BigEndian);

        let mut compressed_input: CompressedInput<Cursor<Vec<u8>>> =
            CompressedInput::new(dtm, ref_interval);
        let mut decoder = compressed_input.arithmetic_coder.as_mut().unwrap();
        let mut contexts = compressed_input.contexts.as_mut().unwrap();

        let decoded_inter_block =
            AduInterBlock::decompress(&mut decoder, &mut contexts, &mut bitreader, dtm);

        decoder.model.set_context(contexts.eof_context);
        let eof = decoder.decode(&mut bitreader).unwrap();
        assert!(eof.is_none());
        assert_eq!(
            reference_block.shift_loss_param,
            decoded_inter_block.shift_loss_param
        );
        assert_eq!(reference_block.d_residuals, decoded_inter_block.d_residuals);
        assert_eq!(reference_block.t_residuals, decoded_inter_block.t_residuals);
    }
}
