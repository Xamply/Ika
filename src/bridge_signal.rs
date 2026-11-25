use std::{path::Path, thread::JoinHandle};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
        System::Threading::{
            CreateEventW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, ResetEvent,
            SYNCHRONIZATION_ACCESS_RIGHTS, SetEvent, WaitForMultipleObjects,
        },
    },
    core::{HSTRING, PCWSTR},
};
use winit::event_loop::EventLoopProxy;

const SYNCHRONIZE: SYNCHRONIZATION_ACCESS_RIGHTS = SYNCHRONIZATION_ACCESS_RIGHTS(0x00100000);

struct Event(usize);

impl Event {
    fn open(folder: &Path, suffix: &str, access: SYNCHRONIZATION_ACCESS_RIGHTS) -> Option<Self> {
        let name = format!(
            "Local\\IkaWallpaper-{}-{suffix}",
            folder.file_name()?.to_str()?
        );
        unsafe { OpenEventW(access, false, &HSTRING::from(name)) }
            .ok()
            .map(|h| Self(h.0 as usize))
    }

    fn handle(&self) -> HANDLE {
        HANDLE(self.0 as _)
    }

    fn set(&self, enabled: bool) -> windows::core::Result<()> {
        unsafe {
            if enabled {
                SetEvent(self.handle())
            } else {
                ResetEvent(self.handle())
            }
        }
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.handle()) };
    }
}

pub struct Wake {
    request: Event,
    stop: Event,
    thread: Option<JoinHandle<()>>,
}

impl Wake {
    pub fn connect(folder: &Path, proxy: EventLoopProxy<()>) -> Option<Self> {
        let request = Event::open(folder, "request", EVENT_MODIFY_STATE)?;
        let ready = Event::open(folder, "ready", SYNCHRONIZE)?;
        let stop = Event(
            unsafe { CreateEventW(None, false, false, PCWSTR::null()) }
                .ok()?
                .0 as usize,
        );
        let quit = stop.0;
        let thread = std::thread::Builder::new()
            .name("wallpaper-ready".into())
            .spawn(move || {
                let handles = [ready.handle(), HANDLE(quit as _)];
                while unsafe { WaitForMultipleObjects(&handles, false, INFINITE) } == WAIT_OBJECT_0
                {
                    if proxy.send_event(()).is_err() {
                        break;
                    }
                }
            })
            .ok()?;
        Some(Self {
            request,
            stop,
            thread: Some(thread),
        })
    }

    pub fn request(&self) {
        let _ = self.request.set(true);
    }
}

impl Drop for Wake {
    fn drop(&mut self) {
        let _ = self.stop.set(true);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// An auto-reset event shared by the GPU fences that can make the window
/// redraw. One waiter is enough for the wallpaper and either preview.
pub struct FrameWake {
    ready: Event,
    stop: Event,
    thread: Option<JoinHandle<()>>,
}

impl FrameWake {
    pub fn new(proxy: EventLoopProxy<()>) -> windows::core::Result<Self> {
        let make = || unsafe { CreateEventW(None, false, false, PCWSTR::null()) }
            .map(|handle| Event(handle.0 as usize));
        let ready = make()?;
        let stop = make()?;
        let (frame, quit) = (ready.0, stop.0);
        let thread = std::thread::spawn(move || {
            let handles = [HANDLE(quit as _), HANDLE(frame as _)];
            while unsafe { WaitForMultipleObjects(&handles, false, INFINITE) }.0
                == WAIT_OBJECT_0.0 + 1
            {
                if proxy.send_event(()).is_err() {
                    break;
                }
            }
        });
        Ok(Self { ready, stop, thread: Some(thread) })
    }

    pub fn handle(&self) -> HANDLE {
        self.ready.handle()
    }
}

impl Drop for FrameWake {
    fn drop(&mut self) {
        let _ = self.stop.set(true);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub struct Audio {
    enabled: Event,
    changed: Option<Event>,
    pub playing: bool,
}

impl Audio {
    pub fn open(folder: &Path, generation: &str) -> Option<Self> {
        Some(Self {
            enabled: Event::open(folder, &format!("audio-{generation}"), EVENT_MODIFY_STATE)?,
            changed: Event::open(
                folder,
                &format!("audio-wake-{generation}"),
                EVENT_MODIFY_STATE,
            ),
            playing: false,
        })
    }

    pub fn set(&mut self, playing: bool) {
        if let Err(error) = self.enabled.set(playing) {
            eprintln!("No se pudo cambiar el sonido del fondo: {error}");
            return;
        }
        self.playing = playing;
        if let Some(changed) = &self.changed {
            let _ = changed.set(true);
        }
    }
}
