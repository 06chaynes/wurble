use std::{f32::consts::TAU, iter::once};

#[cfg(feature = "wasm-bindgen")]
use wasm_bindgen::prelude::wasm_bindgen;

const STOPS: [char; 4] = ['.', '!', '?', '…'];
// First three formants of a, e, i, o, u, in harmonics of one cycle.
const VOWELS: [[f32; 3]; 5] = [
    [4.6, 7.0, 14.2],
    [2.9, 11.0, 14.4],
    [1.7, 13.4, 17.5],
    [2.9, 5.2, 14.0],
    [2.0, 4.6, 13.0],
];

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen)]
#[derive(Clone, Debug)]
pub struct Voice {
    // Sound bank; `a` picks the first clip, `b` the next and so on, wrapping. Short clips loop.
    #[cfg_attr(feature = "wasm-bindgen", wasm_bindgen(skip))]
    pub clips: Vec<Vec<f32>>,
    // Of both the clips and the output.
    pub sample_rate: u32,
    // Playback rate; 1.0 plays clips as recorded.
    pub pitch: f32,
    // Semitones of seeded variation per sound, in either direction.
    pub jitter: f32,
    // Semitones of rise into `?`; `!` lifts its clause by half of it.
    pub inflection: f32,
    // Seconds per character.
    pub tick: f32,
    // Seconds each sound lasts, half again for a sentence's last word.
    pub length: f32,
    // Letters left silent after each sound, starting fresh each word.
    pub skip: usize,
    // Seconds added after `, ; :` and doubled after a stop.
    pub pause: f32,
    // How breathy each consonant starts, from 0 to 1.
    pub hiss: f32,
    pub seed: u64,
}

impl Voice {
    pub fn new(clips: Vec<Vec<f32>>) -> Self {
        Self {
            clips,
            sample_rate: 44_100,
            pitch: 1.0,
            jitter: 0.0,
            inflection: 0.0,
            tick: 0.06,
            length: 0.06,
            skip: 0,
            pause: 0.15,
            hiss: 0.0,
            seed: 0,
        }
    }
}

#[cfg(feature = "wasm-bindgen")]
#[wasm_bindgen]
impl Voice {
    #[wasm_bindgen(constructor)]
    pub fn empty() -> Self {
        Self::new(vec![])
    }

    pub fn add_clip(&mut self, clip: &[f32]) {
        self.clips.push(clip.to_vec());
    }
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen)]
impl Voice {
    pub fn chitter() -> Self {
        Self {
            pitch: 2.0,
            jitter: 1.5,
            inflection: 3.0,
            hiss: 0.3,
            ..Self::new(VOWELS.map(vowel).into())
        }
    }

    pub fn grumble() -> Self {
        Self {
            pitch: 0.7,
            jitter: 4.0,
            inflection: 2.0,
            length: 0.11,
            skip: 1,
            hiss: 0.3,
            ..Self::new(vec![vowel(VOWELS[4])])
        }
    }

    pub fn blip() -> Self {
        Self {
            pitch: 2.5,
            tick: 0.04,
            length: 0.03,
            ..Self::new(vec![wave(|k| (k % 2) as f32 / k as f32)])
        }
    }

    pub fn babble() -> Self {
        Self {
            pitch: 1.1,
            jitter: 3.0,
            inflection: 5.0,
            length: 0.135,
            skip: 2,
            hiss: 0.3,
            ..Self::new(VOWELS.map(vowel).into())
        }
    }

    // Start time of each character in seconds, then the total length.
    pub fn cues(&self, text: &str) -> Vec<f32> {
        let mut at = 0.0;
        once(at)
            .chain(text.chars().map(|c| {
                let pauses = match c {
                    ',' | ';' | ':' => 1.0,
                    c if STOPS.contains(&c) => 2.0,
                    _ => 0.0,
                };
                at += self.tick + self.pause * pauses;
                at
            }))
            .collect()
    }

    // Mono samples in [-1, 1].
    pub fn speak(&self, text: &str) -> Vec<f32> {
        let cues = self.cues(text);
        let at = |seconds: &f32| (seconds * self.sample_rate as f32).round() as usize;
        let mut out = vec![0.0; cues.last().map_or(0, at)];

        // Scanned backwards so each character knows the stop ahead of it, how far off it is
        // and whether its word ends the sentence.
        let mut stop = (' ', 0f32, true);
        let lifts: Vec<(f32, bool)> = text
            .chars()
            .rev()
            .map(|c| {
                if STOPS.contains(&c) {
                    stop = (c, 0.0, true);
                } else {
                    stop.1 += 1.0;
                    stop.2 &= c.is_alphanumeric();
                }
                let lift = match stop.0 {
                    '?' => (1.0 - stop.1 / 8.0).max(0.0),
                    '!' => 0.5,
                    _ => 0.0,
                };
                (lift, stop.2)
            })
            .collect();

        let mut run = 0;
        let sounds = text
            .chars()
            .zip(&cues)
            .zip(lifts.iter().rev())
            .enumerate()
            .filter(|(_, ((c, _), _))| {
                let sounds = c.is_alphanumeric() && run % self.skip.saturating_add(1) == 0;
                run = if c.is_alphanumeric() { run + 1 } else { 0 };
                sounds
            });
        let mut last = None;
        for (i, ((c, cue), (lift, ends))) in sounds {
            let letter = (c.to_ascii_lowercase() as u32).wrapping_sub('a' as u32) as usize;
            let Some(clip) = self
                .clips
                .get(letter % self.clips.len().max(1))
                .filter(|clip| !clip.is_empty() && self.pitch > 0.0)
            else {
                continue;
            };
            let key = self.seed ^ ((i as u64) << 32) ^ c as u64;
            let bend = self.jitter * noise(key) + self.inflection * lift;
            let from = last.replace(bend).unwrap_or(bend);
            let hiss = if "aeiou".contains(c.to_ascii_lowercase()) {
                0.0
            } else {
                self.hiss
            };
            // The last word of a sentence is drawn out.
            let len = at(&(self.length * if *ends { 1.5 } else { 1.0 }));
            let read = |m: f32| {
                let pos = m * self.pitch;
                let j = pos as usize % clip.len();
                clip[j] + (clip[(j + 1) % clip.len()] - clip[j]) * pos.fract()
            };
            let start = at(cue);
            out.resize(out.len().max(start + len), 0.0);
            let mut phase = 0f32;
            for (n, sample) in out[start..start + len].iter_mut().enumerate() {
                let t = n as f32 / len as f32;
                // Slides from the last sound's pitch over the first half. The clip restarts every
                // bent period but always plays at `pitch`, so bends keep the vowel.
                let glide = from + (bend - from) * (2.0 * t).min(1.0);
                let period = (clip.len() as f32 / self.pitch / 2f32.powf(glide / 12.0))
                    .clamp(1.0, len as f32);
                let f = phase.fract();
                phase += 1.0 / period;
                let voiced = read(f * period) * f + read((f + 1.0) * period) * (1.0 - f);
                let b = hiss * (1.0 - t).powi(2);
                // Noise filtered by one cycle of the clip at its loudness and pulsed each period,
                // so breath shares the vowel and pitch.
                let breath = if b > 0.0 {
                    (0..period as usize)
                        .map(|j| noise(key ^ (n + j) as u64) * read(j as f32))
                        .sum::<f32>()
                        * (3.0 / period).sqrt()
                        * if f < 0.5 { 1.0 } else { 0.5 }
                } else {
                    0.0
                };
                *sample += (voiced * (1.0 - b) + breath * b) * (8.0 * t.min(1.0 - t)).min(1.0);
            }
        }
        out.iter_mut().for_each(|s| *s = s.clamp(-1.0, 1.0));
        out
    }

    // A complete 16-bit PCM WAV file.
    pub fn wav(&self, text: &str) -> Vec<u8> {
        let data: Vec<u8> = self
            .speak(text)
            .iter()
            .flat_map(|s| ((s * 32767.0).round() as i16).to_le_bytes())
            .collect();
        let len = data.len() as u32;
        let parts: [&[u8]; 13] = [
            b"RIFF",
            &(36 + len).to_le_bytes(),
            b"WAVEfmt ",
            &16u32.to_le_bytes(),
            &1u16.to_le_bytes(),
            &1u16.to_le_bytes(),
            &self.sample_rate.to_le_bytes(),
            &self.sample_rate.saturating_mul(2).to_le_bytes(),
            &2u16.to_le_bytes(),
            &16u16.to_le_bytes(),
            b"data",
            &len.to_le_bytes(),
            &data,
        ];
        parts.concat()
    }
}

// One cycle built from the amplitude of each harmonic (1-based), at a common loudness that never exceeds 1.
pub fn wave(harmonics: impl Fn(usize) -> f32) -> Vec<f32> {
    let size = 256;
    let cycle: Vec<f32> = (0..size)
        .map(|n| {
            (1..=32)
                .map(|k| harmonics(k) * (TAU * (k * n) as f32 / size as f32).sin())
                .sum()
        })
        .collect();
    let peak = cycle
        .iter()
        .fold(f32::MIN_POSITIVE, |peak, s| peak.max(s.abs()));
    let rms = (cycle.iter().map(|s| s * s).sum::<f32>() / size as f32).sqrt();
    let gain = (0.3 / rms).min(1.0 / peak);
    cycle.iter().map(|s| s * gain).collect()
}

fn vowel(formants: [f32; 3]) -> Vec<f32> {
    wave(|k| {
        formants
            .iter()
            .map(|f| 1.0 / (1.0 + (k as f32 - f).powi(2)))
            .sum::<f32>()
            / k as f32
    })
}

// SplitMix64 finalizer, mapped to [-1, 1].
fn noise(mut x: u64) -> f32 {
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (x ^ (x >> 31)) as f32 / 2f32.powi(63) - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn presets() -> [Voice; 4] {
        [
            Voice::chitter(),
            Voice::grumble(),
            Voice::blip(),
            Voice::babble(),
        ]
    }

    #[test]
    fn cues_follow_ticks_and_pauses() {
        let voice = Voice::blip();
        let cues = voice.cues("a, b.");
        let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
        assert_eq!(cues.len(), 6);
        assert!(close(cues[1], voice.tick));
        assert!(close(cues[2] - cues[1], voice.tick + voice.pause));
        assert!(close(cues[5] - cues[4], voice.tick + voice.pause * 2.0));
    }

    #[test]
    fn audio_spans_the_cues() {
        for voice in presets() {
            let text = "Well, hello there. Is it you?";
            let end = voice.cues(text)[text.chars().count()];
            assert_eq!(voice.speak(text).len(), (end * 44_100.0).round() as usize);
        }
    }

    #[test]
    fn presets_are_audible_and_bounded() {
        for voice in presets() {
            let samples = voice.speak("Hello there! Is that you?");
            assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
            assert!(samples.iter().any(|s| s.abs() > 0.1));
        }
    }

    #[test]
    fn only_letters_and_digits_sound() {
        assert!(
            Voice::chitter()
                .speak("... , !? -")
                .iter()
                .all(|s| *s == 0.0)
        );
        assert!(Voice::new(vec![]).speak("hello").iter().all(|s| *s == 0.0));
        assert!(
            Voice::new(vec![vec![]])
                .speak("hello")
                .iter()
                .all(|s| *s == 0.0)
        );
    }

    #[test]
    fn letters_index_clips_from_a() {
        let mut clips = vec![vec![0.0]; 26];
        clips[0] = vec![1.0];
        let voice = Voice::new(clips);
        assert!(voice.speak("a").iter().any(|s| *s != 0.0));
        assert_eq!(voice.speak("A"), voice.speak("a"));
        assert!(voice.speak("b").iter().all(|s| *s == 0.0));
    }

    #[test]
    fn every_word_sounds() {
        let voice = Voice {
            skip: 9,
            ..Voice::new(vec![vec![1.0]])
        };
        let second = (voice.cues("a b")[2] * 44_100.0).round() as usize;
        assert!(voice.speak("a b")[second..].iter().any(|s| *s != 0.0));
    }

    #[test]
    fn stops_inflect() {
        let voice = Voice::chitter();
        assert_ne!(voice.speak("you?"), voice.speak("you."));
        assert_ne!(voice.speak("you!"), voice.speak("you."));
    }

    #[test]
    fn only_consonants_breathe() {
        let voice = Voice::chitter();
        let calm = Voice {
            hiss: 0.0,
            ..voice.clone()
        };
        assert_ne!(voice.speak("b"), calm.speak("b"));
        assert_eq!(voice.speak("a"), calm.speak("a"));
    }

    #[test]
    fn seed_drives_jitter() {
        let voice = Voice::grumble();
        assert_eq!(voice.speak("hello"), voice.speak("hello"));
        assert_ne!(
            voice.speak("hello"),
            Voice { seed: 1, ..voice }.speak("hello")
        );
    }

    #[test]
    fn wav_header_matches_data() {
        let voice = Voice::blip();
        let (wav, samples) = (voice.wav("hi"), voice.speak("hi").len() as u32);
        assert_eq!(wav.len() as u32, 44 + samples * 2);
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(wav[4..8], (36 + samples * 2).to_le_bytes());
        assert_eq!(wav[24..28], 44_100u32.to_le_bytes());
        assert_eq!(wav[40..44], (samples * 2).to_le_bytes());
    }
}
