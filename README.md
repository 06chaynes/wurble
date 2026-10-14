# wurble

**mumble** **mumble** **mumble**

I don't always warble but when I do I always burble

Mumble speech you can put just about anywhere! (I think... well no, I didn't expect you'd try there)

Human speech is costly and AI speech is ... uncanny (and costly I suppose)? So instead make sounds go with your words that aren't words (also sounds). Who are you and why are you reading this, can you help me, can you release me from this prison, can you feel my heart, is 2 pounds of Nutella too much?

## How to wurble

```sh
cargo add wurble
```

Pick a voice, give it words, get a WAV.

```rust,no_run
use wurble::Voice;

let wav = Voice::chitter().wav("Well, hello there. Is that you?");
std::fs::write("hello.wav", wav).unwrap();
```

### Voices

| Voice | Sounds like | Listen |
|---|---|---|
| `Voice::chitter()` | quick and squeaky, I'm in the crossing for animals | [▶](https://cdn.jsdelivr.net/gh/06chaynes/wurble@main/assets/chitter.wav) |
| `Voice::grumble()` | low grunts, grab a banjo or a kazoo(ie) | [▶](https://cdn.jsdelivr.net/gh/06chaynes/wurble@main/assets/grumble.wav) |
| `Voice::blip()` | one beep per letter, under the night stars... tale (idk either) | [▶](https://cdn.jsdelivr.net/gh/06chaynes/wurble@main/assets/blip.wav) |
| `Voice::babble()` | slow fluent nonsense, the neighbours have a pool but no ladder | [▶](https://cdn.jsdelivr.net/gh/06chaynes/wurble@main/assets/babble.wav) |

### Make it yours

Start from a voice and change whatever bothers you.

```rust
use wurble::Voice;

let deep = Voice { pitch: 0.5, jitter: 6.0, ..Voice::grumble() };
```

| Field | What it does |
|---|---|
| `pitch` | playback rate; `2.0` is an octave up |
| `jitter` | semitones of random wobble per sound |
| `inflection` | semitones of rise into a `?`; a `!` gets half |
| `tick` | time per character |
| `length` | how long each sound lasts |
| `skip` | letters left silent after each sound, starting fresh each word |
| `pause` | silence after `, ; :`, doubled after `. ! ? …` |
| `seed` | same seed, same wobble |

Got your own noises? Hand them over as mono `f32` clips at the voice's `sample_rate`. Each letter picks a clip, so one clip means everything is that noise and 26 in a to z order is an alphabet.

```rust
let mine = Voice::new(vec![my_grunt]);
```

`wave` grows one cycle of sound from the loudness of each harmonic.

```rust
use wurble::{Voice, wave};

let hum = Voice::new(vec![wave(|k| 1.0 / (k * k) as f32)]);
```

### Typewriter text

`cues` says when each character starts, so the words can show up as they're mumbled. The last entry is when the line ends.

```rust
let cues = voice.cues(line);
let shown = cues.iter().filter(|cue| **cue <= elapsed).count();
let visible: String = line.chars().take(shown).collect();
```

### Putting it places

- **A file:** `voice.wav(text)` is a complete WAV file.
- **Bevy:** `AudioSource { bytes: voice.wav(text).into() }`, with Bevy's `wav` feature on.
- **rodio, Kira, cpal, a browser:** `voice.speak(text)` is mono `f32` samples at `voice.sample_rate`.
