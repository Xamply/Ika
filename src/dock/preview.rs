use crate::{
    bridge_signal::Wake,
    gpu::{Gpu, dock_preview::Playback},
    shared::Source,
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

const INTERVAL: Duration = Duration::from_secs(3);

pub struct Bridge<'a> {
    pub folder: &'a Path,
    pub wake: Option<&'a Wake>,
    pub active: usize,
    pub fade: bool,
    pub prepare: bool,
}

#[derive(Default)]
pub struct Preview {
    hovering: bool,
    live: Option<(u64, usize)>,
    requested: Option<(u64, bool, usize)>,
    deadline: Option<Instant>,
    remaining: Option<Duration>,
    initial_attempt: Option<usize>,
    started: bool,
    serial: u64,
    dirty: bool,
}

impl Preview {
    pub fn playback(&self, hover: bool) -> Playback {
        if !hover || !self.hovering {
            Playback::Paused
        } else if self.live.is_some() {
            Playback::Remote
        } else if !self.started {
            // On a very early hover the current wallpaper can move while the
            // independent, pausable preview finishes opening.
            Playback::Main
        } else {
            Playback::Paused
        }
    }

    pub fn leave(&mut self, now: Instant, gpu: &mut Gpu) {
        if self.hovering {
            gpu.dock_preview.pause();
            self.hovering = false;
            if self.requested.is_some_and(|(_, exact, _)| !exact) {
                self.requested = None;
            }
            self.remaining = self
                .deadline
                .take()
                .map(|at| at.saturating_duration_since(now));
            self.dirty = true;
            // Keep the visible worker. Without a consumption fence it parks on
            // its next frame; the thumbnail still shows the last presented one.
        }
    }

    pub fn animating(&self, gpu: &Gpu) -> bool {
        self.hovering && gpu.dock_preview.animating()
    }

    fn request(&mut self, exact: bool, index: usize) {
        self.serial += 1;
        self.requested = Some((self.serial, exact, index));
        self.dirty = true;
    }

    pub fn presented(&mut self, now: Instant, gpu: &Gpu) {
        if let Some((_, index)) = self.live
            && self.hovering
            && self.deadline.is_none()
            && gpu.dock_preview.presented_remote
        {
            self.started = true;
            self.deadline = Some(now + INTERVAL);
            self.request(false, index);
        }
    }

    pub fn poll(
        &mut self,
        now: Instant,
        hover: bool,
        bridge: Option<Bridge<'_>>,
        gpu: &mut Gpu,
    ) -> bool {
        let mut redraw = false;
        if !hover {
            self.leave(now, gpu);
        } else if !self.hovering {
            self.hovering = true;
            if bridge.is_some()
                && let Some((_, index)) = self.live
                && let Some(remaining) = self.remaining.take()
            {
                self.deadline = Some(now + remaining);
                self.request(false, index);
            }
            redraw = true;
        }
        let Some(bridge) = bridge else { return redraw };
        if bridge.prepare && !self.started && self.initial_attempt != Some(bridge.active) {
            // Prepare one frame while the dock is idle. The producer stays
            // parked until hover consumes it, so idle artwork never moves.
            if self.live.take().is_some() {
                gpu.dock_preview.clear_remote();
            }
            self.initial_attempt = Some(bridge.active);
            self.request(true, bridge.active);
        } else if self.hovering && self.live.is_none() && self.requested.is_none()
            && self.deadline.is_none()
        {
            self.initial_attempt = Some(bridge.active);
            self.request(true, bridge.active);
        }
        if let Some((serial, exact, base)) = self.requested
            && (exact || self.deadline.is_some_and(|at| at <= now))
            && let Ok(result) =
                std::fs::read_to_string(bridge.folder.join(format!("dock-result-{serial}.txt")))
        {
            self.requested = None;
            self.dirty = true;
            let _ = std::fs::remove_file(bridge.folder.join(format!("dock-result-{serial}.txt")));
            let (kind, index) = result.trim().split_once(' ').unwrap_or(("skip", ""));
            let index = index.parse().unwrap_or(base);
            match kind {
                "worker" => {
                    let source =
                        Source::read(&bridge.folder.join(format!("surface-dock-{serial}.txt")));
                    match source.and_then(|source| gpu.attach_dock_preview(source, bridge.fade)) {
                        Ok(()) => {
                            self.live = Some((serial, index));
                            self.deadline = None;
                            redraw = true;
                        }
                        Err(error) => {
                            eprintln!("No se pudo abrir la vista previa: {error}");
                            if self.hovering {
                                self.deadline = Some(now + INTERVAL);
                                self.request(false, index);
                            }
                        }
                    }
                }
                _ => {
                    if self.hovering {
                        self.deadline = Some(now + INTERVAL);
                        self.request(false, index);
                    }
                }
            }
        }
        if self.dirty {
            let (serial, exact, index) = self.requested.unwrap_or_default();
            let keep = self.live.map_or(0, |(serial, _)| serial);
            let part = bridge.folder.join("dock-preview.tmp");
            if std::fs::write(
                &part,
                format!("{keep} {serial} {} {index}", u8::from(exact)),
            )
            .and_then(|()| std::fs::rename(part, bridge.folder.join("dock-preview.txt")))
            .is_ok()
            {
                self.dirty = false;
                if let Some(wake) = bridge.wake {
                    wake.request();
                }
            }
        }
        redraw || self.hovering && gpu.dock_preview.remote_ready()
    }

    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        self.deadline.filter(|&at| at > now)
    }

    pub fn needs_poll(&self) -> bool {
        self.dirty || self.requested.is_some()
    }
}
