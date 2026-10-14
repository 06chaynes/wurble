use std::{f32::consts::TAU, iter::once, time::Duration};

const STOPS: [char; 4] = ['.', '!', '?', '…'];
// Formant pairs of a, e, i, o, u, in harmonics of one cycle.
const VOWELS: [[f32; 2]; 5] = [[4.6, 7.0], [2.9, 11.0], [1.7, 13.4], [2.9, 5.2], [2.0, 4.6]];

#[derive(Clone, Debug)]
pub struct Voice {
    // Sound bank; a letter picks its clip by code point, and short clips loop.
    pub clips: Vec<Vec<f32>>,
    // Of both the clips and the output.
    pub sample_rate: u32,
    // Playback rate; 1.0 plays clips as recorded.
    pub pitch: f32,
    // Semitones of seeded variation per sound, in either direction.
    pub jitter: f32,
    // Semitones of rise into `?`; `!` lifts its clause by half of it.
    pub inflection: f32,
    pub tick: Duration,
    pub length: Duration,
    // Letters left silent after each sound.
    pub skip: usize,
    // Added after `, ; :` and doubled after a stop.
    pub pause: Duration,
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
            tick: Duration::from_millis(60),
            length: Duration::from_millis(60),
            skip: 0,
            pause: Duration::from_millis(150),
            seed: 0,
        }
    }

    pub fn chitter() -> Self {
        Self {
            pitch: 2.0,
            jitter: 1.5,
            inflection: 3.0,
            ..Self::new(VOWELS.map(vowel).into())
        }
    }

    pub fn grumble() -> Self {
        Self {
            pitch: 0.7,
            jitter: 4.0,
            inflection: 2.0,
            length: Duration::from_millis(110),
            skip: 1,
            ..Self::new(vec![vowel(VOWELS[4])])
        }
    }

    pub fn blip() -> Self {
        Self {
            pitch: 2.5,
            tick: Duration::from_millis(40),
            length: Duration::from_millis(30),
            ..Self::new(vec![wave(|k| (k % 2) as f32 / k as f32)])
        }
    }

    pub fn babble() -> Self {
        Self {
            pitch: 1.1,
            jitter: 3.0,
            inflection: 5.0,
            length: Duration::from_millis(200),
            skip: 2,
            ..Self::new(VOWELS.map(vowel).into())
        }
    }

    // Start time of each character, then the total length.
    pub fn cues(&self, text: &str) -> Vec<Duration> {
        let mut at = Duration::ZERO;
        once(at)
            .chain(text.chars().map(|c| {
                let pauses = match c {
                    ',' | ';' | ':' => 1,
                    c if STOPS.contains(&c) => 2,
                    _ => 0,
                };
                at += self.tick + self.pause * pauses;
                at
            }))
            .collect()
    }

    // Mono samples in [-1, 1].
    pub fn speak(&self, text: &str) -> Vec<f32> {
        let cues = self.cues(text);
        let at = |d: &Duration| (d.as_secs_f64() * f64::from(self.sample_rate)) as usize;
        let len = at(&self.length);
        let mut out = vec![0.0; cues.last().map_or(0, at)];

        // Scanned backwards so each character knows the stop ahead of it and how far off it is.
        let mut stop = (' ', 0f32);
        let lifts: Vec<f32> = text
            .chars()
            .rev()
            .map(|c| {
                if STOPS.contains(&c) {
                    stop = (c, 0.0);
                } else {
                    stop.1 += 1.0;
                }
                match stop.0 {
                    '?' => (1.0 - stop.1 / 8.0).max(0.0),
                    '!' => 0.5,
                    _ => 0.0,
                }
            })
            .collect();

        let sounds = text
            .chars()
            .zip(&cues)
            .zip(lifts.iter().rev())
            .enumerate()
            .filter(|(_, ((c, _), _))| c.is_alphanumeric())
            .step_by(self.skip.saturating_add(1));
        for (i, ((c, cue), lift)) in sounds {
            let Some(clip) = self
                .clips
                .get(c as usize % self.clips.len().max(1))
                .filter(|clip| !clip.is_empty())
            else {
                continue;
            };
            let bend = self.jitter * noise(self.seed ^ ((i as u64) << 32) ^ c as u64)
                + self.inflection * lift;
            let rate = self.pitch * 2f32.powf(bend / 12.0);
            let start = at(cue);
            out.resize(out.len().max(start + len), 0.0);
            for (n, sample) in out[start..start + len].iter_mut().enumerate() {
                let (pos, t) = (n as f32 * rate, n as f32 / len as f32);
                let j = pos as usize % clip.len();
                let (a, b) = (clip[j], clip[(j + 1) % clip.len()]);
                *sample += (a + (b - a) * pos.fract()) * (8.0 * t.min(1.0 - t)).min(1.0);
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
            .flat_map(|s| ((s * 32767.0) as i16).to_le_bytes())
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

// One cycle built from the amplitude of each harmonic (1-based), peaking at 1.
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
    cycle.iter().map(|s| s / peak).collect()
}

fn vowel(formants: [f32; 2]) -> Vec<f32> {
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
        assert_eq!(cues.len(), 6);
        assert_eq!(cues[1], voice.tick);
        assert_eq!(cues[2] - cues[1], voice.tick + voice.pause);
        assert_eq!(cues[5] - cues[4], voice.tick + voice.pause * 2);
    }

    #[test]
    fn audio_spans_the_cues() {
        for voice in presets() {
            let text = "Well, hello there. Is it you?";
            let end = voice.cues(text)[text.chars().count()].as_secs_f64();
            assert_eq!(voice.speak(text).len(), (end * 44_100.0) as usize);
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
    fn stops_inflect() {
        let voice = Voice::chitter();
        assert_ne!(voice.speak("you?"), voice.speak("you."));
        assert_ne!(voice.speak("you!"), voice.speak("you."));
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
