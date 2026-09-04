use anyhow::{anyhow, Result};
use image::{DynamicImage, ImageBuffer, Rgb};
use ocr_rs::{OcrEngine, OcrEngineConfig};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};

const DET_MODEL: &[u8] = include_bytes!("../rust-paddle-ocr/models/PP-OCRv6_small_det.mnn");
const REC_MODEL: &[u8] = include_bytes!("../rust-paddle-ocr/models/PP-OCRv6_small_rec.mnn");
const CHARSET: &[u8] = include_bytes!("../rust-paddle-ocr/models/ppocr_keys_v6_small.txt");

pub struct OcrLine {
    pub text: String,
    pub score: f32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

enum Msg {
    Run(DynamicImage, Sender<Result<Vec<OcrLine>, String>>),
}

pub struct Ocr {
    tx: Option<Sender<Msg>>,
    handle: Option<JoinHandle<()>>,
}

static GLOBAL: OnceLock<Arc<Ocr>> = OnceLock::new();

impl Ocr {
    pub fn global() -> Result<Arc<Ocr>> {
        if let Some(o) = GLOBAL.get() {
            return Ok(o.clone());
        }
        let o = Arc::new(Ocr::new()?);
        match GLOBAL.set(o.clone()) {
            Ok(()) => Ok(o),
            Err(existing) => Ok(existing.clone()),
        }
    }

    pub fn new() -> Result<Self> {
        let (tx, rx) = channel::<Msg>();
        let (init_tx, init_rx) = channel();
        let handle = thread::Builder::new()
            .name("ocr".into())
            .spawn(move || {
                let engine = match OcrEngine::from_bytes(
                    DET_MODEL,
                    REC_MODEL,
                    CHARSET,
                    Some(OcrEngineConfig::new().with_threads(4)),
                ) {
                    Ok(e) => e,
                    Err(e) => {
                        let _ = init_tx.send(Err(format!("{e:#}")));
                        return;
                    }
                };
                let _ = init_tx.send(Ok(()));
                for msg in rx {
                    match msg {
                        Msg::Run(img, resp) => {
                            let r = engine
                                .recognize(&img)
                                .map(|lines| {
                                    lines
                                        .into_iter()
                                        .map(|l| {
                                            let r = l.bbox.rect;
                                            OcrLine {
                                                text: l.text,
                                                score: l.confidence,
                                                x: r.left() as f32,
                                                y: r.top() as f32,
                                                w: r.width() as f32,
                                                h: r.height() as f32,
                                            }
                                        })
                                        .collect()
                                })
                                .map_err(|e| format!("{e:#}"));
                            let _ = resp.send(r);
                        }
                    }
                }
            })?;
        init_rx
            .recv()
            .map_err(|_| anyhow!("OCR 线程意外退出"))?
            .map_err(anyhow::Error::msg)?;
        Ok(Self { tx: Some(tx), handle: Some(handle) })
    }

    pub fn recognize_roi(
        &self,
        frame: &ImageBuffer<Rgb<u8>, Vec<u8>>,
        roi: (u32, u32, u32, u32),
        scale: bool
    ) -> Result<Vec<OcrLine>> {
        let (x, y, w, h) = if scale {
            crate::vision::scale_roi(frame.width(), frame.height(), roi)
        } else {
            roi
        };
        let crop = image::imageops::crop_imm(frame, x, y, w, h).to_image();
        let (tx, rx) = channel();
        self.tx.as_ref().unwrap().send(Msg::Run(DynamicImage::ImageRgb8(crop), tx))?;
        rx.recv()
            .map_err(|_| anyhow!("OCR 线程已退出"))?
            .map_err(anyhow::Error::msg)
    }
}

impl Drop for Ocr {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
