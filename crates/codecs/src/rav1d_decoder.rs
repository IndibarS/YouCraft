//! Native AV1 backend using rav1d's safe Rust API (user-authorized codec exception).
//! Packet ownership, backpressure and delayed timestamps stay inside this adapter.
use crate::{CodecError, DecodedFrame, Result, VideoDecoder};
use filmcraft_frame::{Chroma, PixelData, VideoFrame};
use rav1d::{Decoder, Picture, PixelLayout, PlanarImageComponent as Plane, Rav1dError, Settings};
use std::sync::Arc;

const MAX_PIXELS: usize = 8192 * 8192;
const MAX_OUTPUT: usize = 1024;

pub struct Rav1dDecoder {
    decoder: Option<Decoder>,
    config: Vec<u8>,
    primed: bool,
    background: bool,
}

impl Rav1dDecoder {
    pub fn new(config: Vec<u8>) -> Result<Self> {
        let background = filmcraft_media::cancel::background();
        Ok(Self { decoder: Some(Self::create(background)?), config, primed: false, background })
    }

    fn create(background: bool) -> Result<Decoder> {
        let mut settings = Settings::new();
        settings.set_n_threads(if background { 1 } else { std::thread::available_parallelism().map_or(2, |n| n.get().clamp(1, 8)) as u32 });
        settings.set_max_frame_delay(if background { 1 } else { 4 });
        settings.set_frame_size_limit(MAX_PIXELS as u32);
        Decoder::with_settings(&settings).map_err(error)
    }

    fn receive(&mut self, out: &mut Vec<DecodedFrame>) -> Result<bool> {
        match self.decoder.as_mut().ok_or_else(|| CodecError::Decode("AV1 decoder is reset".into()))?.get_picture() {
            Ok(p) => {
                if out.len() >= MAX_OUTPUT {
                    return Err(CodecError::Decode("Too many AV1 pictures in one packet".into()));
                }
                out.push(convert(p)?);
                Ok(true)
            }
            Err(Rav1dError::TryAgain) => Ok(false),
            Err(e) => Err(error(e)),
        }
    }

    fn send(&mut self, packet: &[u8], pts: i64, out: &mut Vec<DecodedFrame>) -> Result<()> {
        if packet.is_empty() {
            return Ok(());
        }
        if packet.len() > 64 << 20 {
            return Err(CodecError::Decode("AV1 packet exceeds 64 MB".into()));
        }
        let mut sent = self.decoder.as_mut().ok_or_else(|| CodecError::Decode("AV1 decoder is reset".into()))?.send_data(packet.into(), None, Some(pts), None);
        for _ in 0..MAX_OUTPUT {
            match sent {
                Ok(()) => {
                    self.receive(out)?;
                    return Ok(());
                }
                Err(Rav1dError::TryAgain) => {
                    self.receive(out)?;
                    sent = self.decoder.as_mut().ok_or_else(|| CodecError::Decode("AV1 decoder is reset".into()))?.send_pending_data();
                }
                Err(e) => return Err(error(e)),
            }
        }
        Err(CodecError::Decode("AV1 decoder made insufficient progress".into()))
    }
}

impl VideoDecoder for Rav1dDecoder {
    fn decode(&mut self, sample: &[u8], pts: i64) -> Result<Vec<DecodedFrame>> {
        if self.decoder.is_none() {
            self.decoder = Some(Self::create(self.background)?);
        }
        let mut out = Vec::new();
        let result = (|| {
            if !self.primed {
                let config = self.config.clone();
                self.send(&config, pts, &mut out)?;
                self.primed = true;
            }
            self.send(sample, pts, &mut out)
        })();
        if let Err(e) = result {
            self.reset();
            return Err(e);
        }
        Ok(out)
    }

    fn flush(&mut self) -> Vec<DecodedFrame> {
        // get_picture drains delayed output. Decoder::flush discards reference state, so
        // reserve that operation for reset/seek rather than calling it at end-of-stream.
        let mut out = Vec::new();
        for _ in 0..MAX_OUTPUT {
            match self.receive(&mut out) {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => {
                    log::warn!("rav1d drain failed: {e}");
                    break;
                }
            }
        }
        out
    }

    fn reset(&mut self) {
        self.decoder = None;
        self.primed = false;
    }
    fn thread_limited(&self) -> bool {
        self.background
    }
    fn name(&self) -> &str {
        "rav1d AV1"
    }
    fn is_random_access(&self, sample: &[u8]) -> Option<bool> {
        Some(filmcraft_av1::is_key_frame_unit(sample))
    }
}

fn error(e: Rav1dError) -> CodecError {
    CodecError::Decode(format!("rav1d: {e:?}"))
}

fn plane_bytes(p: &Picture, plane: Plane, w: usize, h: usize, sample_bytes: usize) -> Result<Vec<u8>> {
    let row = w.checked_mul(sample_bytes).ok_or_else(|| CodecError::Decode("AV1 row overflow".into()))?;
    // rav1d's native stride is measured in bytes, including for 10/12-bit pictures.
    let stride = p.stride(plane) as usize;
    let data = p.plane(plane);
    let length = row.checked_mul(h).ok_or_else(|| CodecError::Decode("AV1 plane overflow".into()))?;
    if stride < row || length > MAX_PIXELS * 2 {
        return Err(CodecError::Decode("Invalid AV1 plane dimensions".into()));
    }
    let mut out = Vec::with_capacity(length);
    for y in 0..h {
        let start = y.checked_mul(stride).ok_or_else(|| CodecError::Decode("AV1 stride overflow".into()))?;
        let end = start.checked_add(row).ok_or_else(|| CodecError::Decode("AV1 row overflow".into()))?;
        out.extend_from_slice(data.get(start..end).ok_or_else(|| CodecError::Decode("Truncated AV1 plane".into()))?);
    }
    Ok(out)
}

fn convert(p: Picture) -> Result<DecodedFrame> {
    let (w, h) = (p.width() as usize, p.height() as usize);
    if w == 0 || h == 0 || w.checked_mul(h).is_none_or(|n| n > MAX_PIXELS) {
        return Err(CodecError::Decode("Invalid AV1 picture dimensions".into()));
    }
    let bits = p.bits_per_component().ok_or_else(|| CodecError::Decode("Invalid AV1 bit depth".into()))?.0;
    let (chroma, sx, sy, mono) = match p.pixel_layout() {
        PixelLayout::I400 => (Chroma::C420, 1, 1, true),
        PixelLayout::I420 => (Chroma::C420, 1, 1, false),
        PixelLayout::I422 => (Chroma::C422, 1, 0, false),
        PixelLayout::I444 => (Chroma::C444, 0, 0, false),
    };
    let (cw, ch) = (w.div_ceil(1 << sx), h.div_ceil(1 << sy));
    let b = if bits > 8 { 2 } else { 1 };
    let y = plane_bytes(&p, Plane::Y, w, h, b)?;
    let uv = |plane| -> Result<Vec<u8>> {
        if !mono {
            return plane_bytes(&p, plane, cw, ch, b);
        }
        if b == 1 {
            return Ok(vec![128; cw * ch]);
        }
        let neutral = (1u16 << (bits - 1)).to_ne_bytes();
        Ok((0..cw * ch).flat_map(|_| neutral.into_iter().take(b)).collect())
    };
    let planes = [y, uv(Plane::U)?, uv(Plane::V)?];
    let data = if bits == 8 {
        PixelData::Yuv8 { planes: planes.map(Arc::new), chroma, alpha: None }
    } else {
        PixelData::Yuv16 {
            planes: planes.map(|v| Arc::new(v.chunks_exact(2).map(|x| u16::from_ne_bytes([x[0], x[1]])).collect())),
            chroma,
            bits: bits.into(),
            alpha: None,
        }
    };
    let color = crate::hw_frame::av1_color(
        p.width(),
        p.height(),
        p.matrix_coefficients() as u8,
        p.transfer_characteristic() as u8,
        p.color_primaries() as u8,
        p.color_range() == rav1d::pixel::YUVRange::Full,
    );
    let pts = p.timestamp().ok_or_else(|| CodecError::Decode("AV1 picture has no timestamp".into()))?;
    Ok(DecodedFrame {
        pts,
        frame: VideoFrame { width: p.width(), height: p.height(), data, color, par: (1, 1), pts: filmcraft_time::Tick::ZERO },
        draft: false,
    })
}
