use crate::{CompressSettings, DecompressSettings, Error, Result};
use flate2::Compression;
use flate2::write::{ZlibEncoder, ZlibDecoder};
use std::io::Write;

fn check_zlib_stream(inp: &[u8]) -> Result<(), Error> {
    if inp.len() < 2 {
        return Err(Error::new(53));
    }
    /*read information from zlib header*/
    if (u32::from(inp[0]) * 256 + u32::from(inp[1])) % 31 != 0 {
        /*error: 256 * in[0] + in[1] must be a multiple of 31, the FCHECK value is supposed to be made that way*/
        return Err(Error::new(24));
    }
    let cm = u32::from(inp[0]) & 15;
    let cinfo = (u32::from(inp[0]) >> 4) & 15;
    let fdict = (u32::from(inp[1]) >> 5) & 1;
    if cm != 8 || cinfo > 7 {
        /*error: only compression method 8: inflate with sliding window of 32k is supported by the PNG spec*/
        return Err(Error::new(25));
    }
    if fdict != 0 {
        /*error: the specification of PNG says about the zlib stream:
          "The additional flags shall not specify a preset dictionary."*/
        return Err(Error::new(26));
    }

    Ok(())
}

pub(crate) enum Decoder<'settings> {
    Flate(ZlibDecoder<Vec<u8>>),
    Custom(&'settings DecompressSettings, Vec<u8>, Vec<u8>),
}

impl Decoder<'_> {
    pub fn push(&mut self, chunk: &[u8]) -> Result<(), Error> {
        match self {
            Self::Flate(dec) => {
                dec.write_all(chunk).map_err(|_| Error::new(23))?;
            },
            Self::Custom(_, buf, _) => {
                buf.extend_from_slice(chunk);
            },
        }
        Ok(())
    }

    pub fn finish(self) -> Result<Vec<u8>, Error> {
        match self {
            Self::Flate(dec) => {
                Ok(dec.finish().map_err(|_| Error::new(23))?)
            },
            Self::Custom(settings, buf, mut out) => {
                check_zlib_stream(&buf)?;

                out.try_reserve((buf.len() * 3 / 2).max(16*1024))?;
                let cb = settings.custom_zlib.ok_or(Error::new(87))?; // can't fail
                (cb)(&buf, &mut out, settings)?;
                Ok(out)
            }
        }
    }
}

pub(crate) fn new_decompressor<'w>(out: Vec<u8>, zlib_data_size: usize, settings: &DecompressSettings) -> Decoder<'_> {
    if settings.custom_zlib.is_some() {
        let mut buf = Vec::new();
        let _ = buf.try_reserve_exact(zlib_data_size);
        Decoder::Custom(settings, buf, out)
    } else {
        Decoder::Flate(ZlibDecoder::new(out))
    }
}

#[inline(never)]
pub(crate) fn decompress_into_vec(inp: &[u8]) -> Result<Vec<u8>, Error> {
    check_zlib_stream(inp)?;
    let mut out = Vec::new();
    out.try_reserve((inp.len() * 3 / 2).max(16*1024))?;
    let mut dec = ZlibDecoder::new(out);
    dec.write_all(inp).map_err(|_| Error::new(23))?;
    dec.finish().map_err(|_| Error::new(23))
}

pub(crate) fn decompress(inp: &[u8], settings: &DecompressSettings) -> Result<Vec<u8>, Error> {
    if let Some(cb) = settings.custom_zlib {
        let mut out = Vec::new(); out.try_reserve(inp.len() * 3 / 2)?;
        (cb)(inp, &mut out, settings)?;
        Ok(out)
    } else {
        decompress_into_vec(inp)
    }
}

pub(crate) fn new_compressor<W: Write>(outv: W, settings: &CompressSettings) -> Result<ZlibEncoder<W>, Error> {
    let level = settings.level();
    let level = if level == 0 {
        Compression::none()
    } else {
        Compression::new(level.min(9).into())
    };
    Ok(ZlibEncoder::new(outv, level))
}

#[inline(never)]
pub(crate) fn compress_into(out: &mut dyn Write, inp: &[u8], settings: &CompressSettings) -> Result<(), Error> {

    #[allow(deprecated)]
    if let Some(cb) = settings.custom_zlib {
        (cb)(inp, out, settings)?;
    } else {
        let mut z = new_compressor(out, settings)?;
        z.write_all(inp)?;
    }
    Ok(())
}


pub(crate) fn compress_fast(source: &[u8], destination: &mut Vec<u8>) {
    let mut zlib = ZlibEncoder::new(destination, Compression::new(1));
    let _ = zlib.write_all(source);
}
