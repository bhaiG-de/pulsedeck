# Native time/pitch DSP

Unmodified MIT sources pinned on 2026-10-14:

* Signalsmith Stretch 1.3.2, https://github.com/Signalsmith-Audio/signalsmith-stretch,
  commit `a670068d9aeb64913331d5cc29337b19a457a7df`.
* Signalsmith Linear, https://github.com/Signalsmith-Audio/linear,
  commit `146b26f91b48835d665fa0498d64d9617818d27a`.

Only the required headers and complete license notices are vendored. Portable FFTs
keep platform dependencies unchanged. Build optimization is enabled for this DSP
bridge even in debug builds, as recommended by the upstream documentation.
