use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use notify::{recommended_watcher, Event, RecursiveMode, Result as NotifyResult, Watcher};

pub enum DirEvent {
    Created(PathBuf),
    Removed(PathBuf),
    Modified(PathBuf),
}

pub struct DirWatcher {
    _watcher: Box<dyn Watcher>,
    rx: Receiver<NotifyResult<Event>>,
}

impl DirWatcher {
    pub fn new(path: &Path) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = recommended_watcher(tx)?;
        watcher.watch(path, RecursiveMode::NonRecursive)?;
        Ok(Self { _watcher: Box::new(watcher), rx })
    }

    /// Drain pending filesystem events; call once per frame.
    pub fn poll(&self) -> Vec<DirEvent> {
        let mut events = Vec::new();
        while let Ok(ev_result) = self.rx.try_recv() {
            let Ok(ev) = ev_result else { continue };
            use notify::EventKind;
            for path in ev.paths {
                if !is_fits(&path) {
                    continue;
                }
                let dir_ev = match ev.kind {
                    EventKind::Create(_) => DirEvent::Created(path),
                    EventKind::Remove(_) => DirEvent::Removed(path),
                    EventKind::Modify(_) => DirEvent::Modified(path),
                    _ => continue,
                };
                events.push(dir_ev);
            }
        }
        events
    }
}

fn is_fits(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".fits") || lower.ends_with(".fit") || lower.ends_with(".fits.gz")
}
