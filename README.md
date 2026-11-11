# Sky Lyrics for Horizon

A native Horizon mod (Minecraft 26.3, Horizon `fdc87ae`) that floats the words of
the song you are playing in the sky, in real 3D: hills, trees and clouds pass in
front of them. Written from scratch against Horizon's source.

- **K** opens the settings. The world stays visible and every change shows live.
- **J** shows or hides the lyrics.
- Both keys can be rebound in Options > Mods.

## Where the music comes from

Anything that reports to the desktop's media controls through MPRIS: Spotify, a
browser, mpv and others, read with `playerctl`, which has to be installed. If
several players are open the mod follows the one playing, Spotify first. The Music
tab can pin one.

Lyrics are looked for in this order:

1. Your own files: `config/sky_lyrics/lyrics/Artist - Title.lrc` or `Title.lrc`.
   Word tags (`<00:12.30>`) are used when present.
2. The mod's cache: `config/sky_lyrics/cache`.
3. [LRCLIB](https://lrclib.net), a free lyric database. The song's artist, title,
   album and length are sent to it. Turn off **Look up lyrics online** on the Music
   tab to keep it all local.

LRCLIB times lines, not words, so word by word timing is estimated from each
word's length unless the `.lrc` has word tags. If the words run early or late, a
browser may be reporting its position badly: adjust **Timing offset**.

## Fonts

**Text > Font** lists every font on the system, searchable by typing. Put your own
`.ttf` or `.otf` files in `config/sky_lyrics/fonts` and they appear first. Letters
the chosen font lacks come from Noto or DejaVu when installed.

Text is laid out left to right with kerning but no shaping, so scripts that need
joined letters (Arabic, for one) will not join.

## Settings

Everything is on the K screen and in `config/sky_lyrics.cfg`.

| Tab | What it changes |
| --- | --- |
| Place | Anchor (`follow`, `compass`, `locked`), height, left/right, follow speed and dead zone, facing, curve, tilt, lean, float |
| Text | Font, size, weight, slant, letter/word spacing, line height, wrap width, alignment, case |
| Colour | Sky-matching, solid, gradient or rainbow; sung word colour; opacity; outline; glow; shadow |
| Motion | Reveal by word, letter, line or karaoke; entrance and exit style and time; word pop; wave; surrounding lines; gap hiding; song title card |
| Music | Player, timing offset, online lookup, paused behaviour, lyrics with no timing, a line on the HUD |
| Styles | Ready-made looks to start from |

## Limits

The words are drawn with the sky, so they show in the Overworld with the sky on.
They are not drawn in the Nether or the End, under Blindness or Darkness, or when
a shader pack draws its own sky. The HUD line on the Music tab keeps working in
all of those.

## Build

Pinned to Horizon `fdc87ae`, because it draws inside the renderer's sky pass.

```
cargo test --offline
python3 tools/package.py --install
```

`--install` copies `sky_lyrics.hnmod` into `~/.local/share/horizon/horizon-native-mods`.
Then add `mod.sky_lyrics:true` to `horizon-mods.txt` with the game closed, and
rebuild with the launcher's `native-build`.

## Layout

- `mixins/sky_lyrics/engine`: settings, LRC timing, player and lookup, glyph atlas, layout and animation, the settings screen. No game types, and the tests.
- `mixins/sky_lyrics/runtime.rs`: the GPU side and the state the hooks share.
- `mixins/sky_lyrics/*.rs`: one small hook each, listed in `horizon-mixin.toml`.
