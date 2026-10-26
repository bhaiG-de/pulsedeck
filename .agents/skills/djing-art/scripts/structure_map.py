#!/usr/bin/env python3
"""Summarize saved track.inspect evidence; never call the app or infer section labels."""
import argparse
import json
import math
from pathlib import Path
import sys

BANDS = ("low", "mid", "high")


def read_inspection(path):
    data = json.loads(Path(path).read_text())
    if data.get('jsonrpc')=='2.0':
        if data.get('error'):raise ValueError(f'{path}: failed MCP request')
        data=data['result']
    if data.get('isError'):raise ValueError(f'{path}: unsuccessful MCP tool result')
    if 'structuredContent' in data:data=data['structuredContent']
    elif 'result' in data:
        if data.get('ok') is not True:raise ValueError(f'{path}: unsuccessful inspection')
        data=data['result']
    if not isinstance(data, dict) or not data.get("source"):
        raise ValueError(f"{path}: expected track.inspect object with source")
    return data


def number(value, label):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise ValueError(f"{label}: expected finite number")
    return float(value)


def identity(data):
    meta = data.get("metadata") or {}
    return {"source": data.get("source"), "source_fingerprint": meta.get("source_fingerprint"),
            "analysis_version": meta.get("analysis_version"), "grid_revision": data.get("grid_revision"),
            "load_revision": data.get("load_revision"),
            "beat_grid_revision": (data.get("beat_events") or {}).get("grid_revision"),
            "event_source": (data.get("beat_events") or {}).get("source")}


def summarize(overview, extras=(), bars=4, beats_per_bar=None, origin_beat=None):
    documents = [(str(overview), read_inspection(overview))]
    documents.extend((str(p), read_inspection(p)) for p in extras)
    primary = documents[0][1]
    reference = {}
    for filename, data in documents:
        outer_revision = data.get("grid_revision")
        event_revision = (data.get("beat_events") or {}).get("grid_revision")
        if outer_revision is not None and event_revision is not None and outer_revision != event_revision:
            raise ValueError(f"{filename}: inconsistent grid revisions within inspection")
        for key, value in identity(data).items():
            if value is not None:
                if key in reference and reference[key] != value:
                    raise ValueError(f"{filename}: mismatching {key}")
                reference[key] = value
    rhythm = (primary.get("metadata") or {}).get("neural_rhythm") or {}
    meter = beats_per_bar if beats_per_bar is not None else rhythm.get("beats_per_bar")
    if not isinstance(meter, int) or isinstance(meter, bool) or meter < 1:
        raise ValueError("supply --beats-per-bar when inspection has no usable meter")
    if bars not in (4, 8):
        raise ValueError("bars must be 4 or 8")
    block_beats = bars * meter
    events = {}
    provenance = []
    for filename, data in documents:
        evidence = data.get("beat_events") or {}
        for event in evidence.get("events") or []:
            position = number(event.get("beat_position"), "beat_position")
            seconds = number(event.get("seconds"), "event seconds")
            old = events.get(position)
            if old and (abs(old["seconds"] - seconds) > 1e-4 or old["downbeat"] != bool(event.get("downbeat"))):
                raise ValueError(f"{filename}: conflicting event at effective beat {position}")
            events[position] = {"seconds": seconds, "downbeat": bool(event.get("downbeat"))}
        provenance.append({"file": filename, "requested_range": data.get("requested_range"),
                           "supplied_events": len(evidence.get("events") or []),
                           "total_in_range": evidence.get("total_in_range"),
                           "truncated": evidence.get("truncated"), "event_source": evidence.get("source"),
                           "local_tempo": evidence.get("local_tempo")})
    positions = sorted(events)
    gaps = []
    segments = []
    if positions:
        start = previous = positions[0]
        for position in positions[1:]:
            if events[position]["seconds"] <= events[previous]["seconds"]:
                raise ValueError("effective events must increase in both position and time")
            if not math.isclose(position - previous, 1.0, abs_tol=1e-7):
                gaps.append({"after_beat": previous, "before_beat": position,
                             "after_seconds": events[previous]["seconds"], "before_seconds": events[position]["seconds"]})
                segments.append({"start_beat": start, "end_beat": previous})
                start = position
            previous = position
        segments.append({"start_beat": start, "end_beat": previous})
    downbeats = [p for p in positions if events[p]["downbeat"]]
    if origin_beat is None:
        if not downbeats:
            raise ValueError("no observed downbeat; supply a reviewed --origin-beat")
        origin = downbeats[0]
        origin_basis = "first supplied effective downbeat; counting origin only"
    else:
        origin = number(origin_beat, "origin beat")
        origin_basis = "explicit counting origin; not a verified phrase origin"
    bins = []
    for item in primary.get("bins") or []:
        start = number(item.get("start_seconds"), "bin start")
        end = number(item.get("end_seconds"), "bin end")
        if end <= start or any(number(item.get(band), f"bin {band}") < 0 for band in BANDS):
            raise ValueError("invalid waveform bin")
        if bins and start < bins[-1]["end_seconds"] - 1e-7:
            raise ValueError("overview waveform bins overlap or are unsorted")
        bins.append(item)
    measurement = (primary.get("metadata") or {}).get("audio_measurements") or {}
    gain_db = measurement.get("normalization_gain_db")
    gain = 10 ** (number(gain_db, "normalization gain") / 20) if gain_db is not None else None
    blocks = []
    omitted = []
    if positions:
        first = math.ceil((positions[0] - origin) / block_beats)
        last = math.floor((positions[-1] - origin) / block_beats) - 1
        for index in range(first, last + 1):
            start_pos = origin + index * block_beats
            end_pos = start_pos + block_beats
            expected = [start_pos + offset for offset in range(block_beats + 1)]
            if not all(pos in events for pos in expected):
                omitted.append({"start_beat": start_pos, "end_beat": end_pos, "reason": "missing effective events"})
                continue
            start = events[start_pos]["seconds"]
            end = events[end_pos]["seconds"]
            sums = dict.fromkeys(BANDS, 0.0)
            covered = 0.0
            peak = None
            for item in bins:
                overlap = max(0.0, min(end, item["end_seconds"]) - max(start, item["start_seconds"]))
                if overlap:
                    covered += overlap
                    for band in BANDS:
                        sums[band] += item[band] ** 2 * overlap
                    if item.get("peak") is not None:
                        peak = max(peak or 0.0, number(item["peak"], "bin peak"))
            raw = {band: math.sqrt(value / covered) if covered else None for band, value in sums.items()}
            normalized = {band: value * gain if value is not None and gain is not None else None for band, value in raw.items()}
            block = {"start_beat": start_pos, "end_beat": end_pos, "start_seconds": start, "end_seconds": end,
                     "start_downbeat": events[start_pos]["downbeat"], "end_downbeat": events[end_pos]["downbeat"],
                     "waveform_coverage_fraction": min(1.0, covered / (end - start)),
                     "rms_raw": raw, "rms_normalized": normalized, "peak_raw_max": peak,
                     "peak_normalized_max": peak * gain if peak is not None and gain is not None else None,
                     "delta_db_from_previous": None}
            if blocks and blocks[-1]["end_beat"] == start_pos and block["waveform_coverage_fraction"] > .999999 and blocks[-1]["waveform_coverage_fraction"] > .999999:
                previous = blocks[-1]["rms_raw"]
                block["delta_db_from_previous"] = {band: 20 * math.log10(raw[band] / previous[band]) if raw[band] and previous[band] else None for band in BANDS}
            blocks.append(block)
    return {"schema": "structure-map-v1", "identity": reference,
            "track": {key: (primary.get("metadata") or {}).get(key) for key in ("title", "artist", "bpm", "key", "camelot", "duration_seconds")},
            "counting": {"bars_per_block": bars, "beats_per_bar": meter, "origin_beat": origin, "origin_basis": origin_basis,
                         "phrase_origins_verified": False},
            "measurements": {"normalization_gain_db": gain_db, "integrated_lufs": measurement.get("integrated_lufs"),
                             "method": "time-weighted mean square of overview bin RMS; partial bins assumed uniform",
                             "scope": "source envelope with measurement gain; actual playback normalization depends on engine state; excludes faders/EQ/FX/tempo and does not predict summed audio"},
            "coverage": {"unique_events": len(events), "segments": segments, "gaps": gaps, "omitted_blocks": omitted,
                         "waveform_file": str(overview), "events_truncated_in_any_input": any(p["truncated"] for p in provenance)},
            "inputs": provenance, "blocks": blocks}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("overview", type=Path)
    parser.add_argument("--events", type=Path, action="append", default=[], help="additional bounded track.inspect file; events only")
    parser.add_argument("--bars", type=int, choices=(4, 8), default=4)
    parser.add_argument("--beats-per-bar", type=int)
    parser.add_argument("--origin-beat", type=float, help="explicit effective beat counting origin, not a phrase assertion")
    args = parser.parse_args()
    try:
        output = summarize(args.overview, args.events, args.bars, args.beats_per_bar, args.origin_beat)
    except (ValueError, OSError, KeyError, TypeError) as error:
        parser.exit(2, f"structure_map: {error}\n")
    json.dump(output, sys.stdout, indent=2, allow_nan=False)
    print()


if __name__ == "__main__":
    main()
