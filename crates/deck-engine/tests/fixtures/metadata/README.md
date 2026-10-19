These tiny fixtures contain a generated 40 ms, 440 Hz stereo sine tone.
FFmpeg wrote the same title, artist, genre and UTF-8 comment into MP3,
FLAC, Ogg Vorbis, MP4/AAC, WAV/RIFF and AIFF/ID3 containers. MP3, FLAC
and Ogg also contain a custom `DJ_ENERGY=Peak 9` text field.

Tests read these files without changing them. Footer tags, multivalue
custom ID3 fields and malformed/stacked metadata are created only in
temporary copies. No downloaded or user-owned music is included.
