#include "signalsmith-stretch.h"
#include <new>
#include <memory>

// One instance per deck. Configure only at engine creation, never in rendering.
// All library scratch vectors retain their configured capacity. The bridge's
// maximum input block/seek lengths fit those capacities; no dynamic containers
// or callbacks are constructed in the processing path.
using Stretch = signalsmith::stretch::SignalsmithStretch<float>;
extern "C" {
void *pd_stretch_new() noexcept {
    try {
        auto stretch = std::make_unique<Stretch>(0);
        // 64 ms spectral window, 16 ms interval at 48k. Split the spectral work
        // over smaller calls; input/output audio rate stays the negotiated rate.
        stretch->configure(2, 3072, 768, true);
        return stretch.release();
    } catch (...) { return nullptr; }
}
void pd_stretch_delete(void *handle) noexcept { delete static_cast<Stretch *>(handle); }
int pd_stretch_input_latency(void *handle) noexcept { return static_cast<Stretch *>(handle)->inputLatency(); }
int pd_stretch_output_latency(void *handle) noexcept { return static_cast<Stretch *>(handle)->outputLatency(); }
int pd_stretch_seek_length(void *handle, float rate) noexcept { return static_cast<Stretch *>(handle)->outputSeekLength(rate); }
void pd_stretch_pitch(void *handle, float factor) noexcept { static_cast<Stretch *>(handle)->setTransposeFactor(factor); }
void pd_stretch_seek(void *handle, const float *left, const float *right, int frames) noexcept {
    const float *input[] = {left, right};
    static_cast<Stretch *>(handle)->outputSeek(input, frames);
}
void pd_stretch_process(void *handle, const float *left, const float *right, int input_frames,
                        float *out_left, float *out_right, int output_frames) noexcept {
    const float *input[] = {left, right}; float *output[] = {out_left, out_right};
    static_cast<Stretch *>(handle)->process(input, input_frames, output, output_frames);
}
}
