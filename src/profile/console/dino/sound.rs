//! The Web Audio oscillator envelopes from startHcpDinoGame, through WinMM.
use std::f64::consts::TAU;
use windows::{
    Win32::Media::Audio::{
        HWAVEOUT, MIDI_WAVE_OPEN_TYPE, WAVE_MAPPER, WAVEFORMATEX, WAVEHDR,
        waveOutClose, waveOutOpen, waveOutPrepareHeader, waveOutReset,
        waveOutUnprepareHeader, waveOutWrite,
    },
    core::PSTR,
};

const RATE: u32 = 48_000;
const HEADER: u32 = std::mem::size_of::<WAVEHDR>() as u32;

#[derive(Clone, Copy)]
pub(super) enum Cue { Jump, Hit, Score }

fn samples(cue: Cue, volume: u8) -> Vec<i16> {
    let (duration, initial, target, decay_ratio, peak): (f64, f64, f64, f64, f64) = match cue {
        Cue::Jump => (0.07, 320., 640., 0.001 / 0.06, 2_200.),
        Cue::Hit => (0.16, 160., 40., 0.001 / 0.10, 3_400.),
        Cue::Score => (0.14, 580., 880., 0.001 / 0.05, 1_800.),
    };
    // Band-limit the square/saw edges (Web Audio's built-in oscillators
    // also avoid aliasing), keeping their phase and exponential ramps.
    fn blep(phase: f64, step: f64) -> f64 {
        if phase < step {
            let t = phase / step;
            2. * t - t * t - 1.
        } else if phase > 1. - step {
            let t = (phase - 1.) / step;
            t * t + 2. * t + 1.
        } else {
            0.
        }
    }
    let mut phase: f64 = 0.;
    let total_samples = (duration * f64::from(RATE)).round() as u32;
    (0..total_samples)
        .map(|i| {
            let t = f64::from(i) / f64::from(RATE);
            let freq = if matches!(cue, Cue::Score) {
                if t < 0.06 { initial } else { target }
            } else {
                initial * (target / initial).powf(t / duration)
            };
            let step = freq / f64::from(RATE);
            let value = match cue {
                Cue::Jump => {
                    (if phase < 0.5 { 1. } else { -1. }) + blep(phase, step)
                        - blep((phase + 0.5).fract(), step)
                }
                Cue::Hit => 1. - 2. * phase + blep(phase, step),
                Cue::Score => (phase * TAU).sin(),
            };
            phase = (phase + step).fract();
            let envelope = decay_ratio.powf(t / duration);
            (value * envelope * (f64::from(volume) / 100.) * peak).round() as i16
        })
        .collect()
}

struct Voice {
    output: HWAVEOUT,
    headers: [Box<WAVEHDR>; 2],
    buffers: [Vec<i16>; 2],
    prepared: [bool; 2],
    idx: usize,
}

impl Voice {
    fn new() -> Option<Self> {
        let mut output = HWAVEOUT::default();
        let format = WAVEFORMATEX {
            wFormatTag: 1,
            nChannels: 1,
            nSamplesPerSec: RATE,
            nAvgBytesPerSec: RATE * 2,
            nBlockAlign: 2,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        let status = unsafe {
            waveOutOpen(Some(&mut output), WAVE_MAPPER, &format, None, None, MIDI_WAVE_OPEN_TYPE(0))
        };
        (status == 0).then(|| Self {
            output,
            headers: [Box::default(), Box::default()],
            buffers: [Vec::new(), Vec::new()],
            prepared: [false, false],
            idx: 0,
        })
    }

    fn play(&mut self, pcm: Vec<i16>) {
        unsafe {
            let _ = waveOutReset(self.output);
            for i in 0..2 {
                if self.prepared[i] {
                    let _ = waveOutUnprepareHeader(self.output, &mut *self.headers[i], HEADER);
                    self.prepared[i] = false;
                }
            }
        }
        let i = self.idx;
        self.idx = 1 - self.idx;
        self.buffers[i] = pcm;
        *self.headers[i] = WAVEHDR {
            lpData: PSTR(self.buffers[i].as_mut_ptr().cast()),
            dwBufferLength: (self.buffers[i].len() * 2) as u32,
            ..Default::default()
        };
        unsafe {
            if waveOutPrepareHeader(self.output, &mut *self.headers[i], HEADER) == 0 {
                self.prepared[i] = true;
                waveOutWrite(self.output, &mut *self.headers[i], HEADER);
            }
        }
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        unsafe {
            waveOutReset(self.output);
            for i in 0..2 {
                if self.prepared[i] {
                    let _ = waveOutUnprepareHeader(self.output, &mut *self.headers[i], HEADER);
                }
            }
            waveOutClose(self.output);
        }
    }
}

#[derive(Default)]
pub(super) struct Audio([Option<Voice>; 3]);
impl Audio {
    pub(super) fn play(&mut self, cue: Cue, volume: u8) {
        if volume == 0 { return; }
        let voice = &mut self.0[cue as usize];
        if voice.is_none() { *voice = Voice::new(); }
        if let Some(voice) = voice { voice.play(samples(cue, volume)); }
    }
    pub(super) fn stop(&mut self) { self.0 = Default::default(); }
}
