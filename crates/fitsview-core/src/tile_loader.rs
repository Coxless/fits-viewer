use std::collections::HashSet;
use std::sync::mpsc;
use std::sync::Arc;

use tokio::runtime::Runtime;

use crate::mmap_reader::MmapFitsImage;
use crate::tile_manager::{TileData, TileKey};

pub struct TileRequest {
    pub key: TileKey,
    pub source: Arc<MmapFitsImage>,
}

pub struct TileResult {
    pub key: TileKey,
    pub result: Result<TileData, String>,
}

pub struct TileLoader {
    request_tx: tokio::sync::mpsc::Sender<TileRequest>,
    result_rx: mpsc::Receiver<TileResult>,
    in_flight: HashSet<TileKey>,
    pub pending: usize,
}

impl TileLoader {
    pub fn new(rt: &Runtime) -> Self {
        let (request_tx, mut request_rx) = tokio::sync::mpsc::channel::<TileRequest>(512);
        let (result_tx, result_rx) = mpsc::sync_channel::<TileResult>(1024);

        rt.spawn(async move {
            while let Some(req) = request_rx.recv().await {
                let tx = result_tx.clone();
                let key = req.key.clone();
                let source = req.source.clone();
                let tx2 = key.tx;
                let ty2 = key.ty;
                tokio::task::spawn_blocking(move || {
                    let zoom_level = key.zoom_level;
                    let result = source
                        .read_tile_lod(tx2, ty2, zoom_level)
                        .map(|pixels| {
                            let (w, h) = source.tile_dims_lod(tx2, ty2, zoom_level);
                            TileData { pixels, width: w, height: h }
                        })
                        .map_err(|e| e.to_string());
                    let _ = tx.try_send(TileResult { key, result });
                });
            }
        });

        Self { request_tx, result_rx, in_flight: HashSet::new(), pending: 0 }
    }

    /// Queue a tile for loading. No-op if already in-flight.
    pub fn request(&mut self, req: TileRequest) {
        if self.in_flight.contains(&req.key) {
            return;
        }
        self.in_flight.insert(req.key.clone());
        self.pending += 1;
        let _ = self.request_tx.try_send(req);
    }

    /// Drain completed tiles; call once per frame.
    pub fn poll_completed(&mut self) -> Vec<TileResult> {
        let mut results = Vec::new();
        while let Ok(r) = self.result_rx.try_recv() {
            self.in_flight.remove(&r.key);
            self.pending = self.pending.saturating_sub(1);
            results.push(r);
        }
        results
    }
}
