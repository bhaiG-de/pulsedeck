#!/usr/bin/env python3
"""Exercise the offline helpers with captured MCP JSON, without a running app."""
import importlib.util
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


SCRIPTS = Path(__file__).resolve().parents[1] / '.agents/skills/djing-art/scripts'


def load(name):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


passage = load('passage_map')
structure = load('structure_map')
phrase = load('phrase_check')


def packet():
    return {
        'source': '/music/record.wav', 'grid_revision': 7,
        'metadata': {
            'title': 'Record', 'bpm': 120, 'bpm_source': 'neural',
            'source_fingerprint': 'source-identity', 'analysis_version': 4,
            'neural_rhythm': {'beats_per_bar': 4, 'bpm': 120},
            'audio_measurements': {'normalization_gain_db': -6},
        },
        'requested_range': {'start_seconds': 0, 'end_seconds': 16},
        'beat_events': {
            'grid_revision': 7, 'source': 'neural', 'truncated': False,
            'total_in_range': 33, 'events': [
                {'seconds': n / 2, 'beat_position': n, 'beat_index': n + 2,
                 'downbeat': n % 4 == 0} for n in range(33)
            ],
        },
        'bins': [{'start_seconds': 0, 'end_seconds': 16,
                  'low': .2, 'mid': .1, 'high': .05, 'peak': .4}],
    }


class OfflineEvidence(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def write(self, name, value):
        path = self.root / name
        path.write_text(json.dumps(value))
        return path

    def test_accepts_direct_structured_and_jsonrpc_captures(self):
        value = packet()
        for i, capture in enumerate((value, {'structuredContent': value},
                {'jsonrpc': '2.0', 'id': 1, 'result': {'structuredContent': value}})):
            self.assertEqual(structure.read_inspection(self.write(str(i), capture)), value)
            self.assertEqual(phrase.payload(capture), value)

    def test_rejects_failed_mcp_capture(self):
        for capture in ({'isError': True, 'structuredContent': packet()},
                        {'jsonrpc': '2.0', 'id': 1, 'error': {'code': -1}}):
            with self.assertRaises(ValueError):
                structure.read_inspection(self.write('failed', capture))
            with self.assertRaises(ValueError):
                phrase.payload(capture)

    def test_map_preserves_exact_events_and_measurement_scope(self):
        value = packet()
        result = passage.build(self.write('overview', {'structuredContent': value}), [], self.root / 'out')
        self.assertEqual(len(result['blocks']), 2)
        self.assertEqual(result['blocks'][0]['start_event'], value['beat_events']['events'][0])
        self.assertEqual(result['blocks'][0]['end_event'], value['beat_events']['events'][16])
        self.assertFalse(result['counting']['phrase_origins_verified'])
        self.assertIn('does not predict summed audio', result['measurements']['scope'])
        self.assertTrue((self.root / 'out/structure-map.json').is_file())

    def test_truncated_range_requires_all_captured_events(self):
        value = packet()
        value['beat_events']['truncated'] = True
        value['beat_events']['events'] = value['beat_events']['events'][:17]
        overview = self.write('overview', value)
        with self.assertRaisesRegex(ValueError, 'incomplete'):
            passage.build(overview, [], self.root / 'missing')
        extra = packet()
        extra['beat_events']['events'] = extra['beat_events']['events'][16:]
        extra['beat_events']['total_in_range'] = 17
        extra['requested_range']['start_seconds'] = 8
        result = passage.build(overview, [self.write('extra', extra)], self.root / 'complete')
        self.assertEqual(result['coverage']['unique_events'], 33)
        self.assertEqual(len(result['blocks']), 2)

    def test_mismatched_source_and_grid_generations_reject(self):
        overview = self.write('overview', packet())
        for field, value in (('source', '/music/other.wav'), ('grid_revision', 8)):
            extra = packet()
            extra[field] = value
            with self.assertRaises(ValueError):
                passage.build(overview, [self.write('extra', extra)], self.root / field)

    def test_missing_meter_keeps_counts_without_claiming_bars(self):
        value = packet()
        value['metadata']['neural_rhythm'] = {}
        result = passage.build(self.write('overview', value), [], self.root / 'out')
        self.assertIsNone(result['counting']['beats_per_bar'])
        self.assertIsNone(result['counting']['bars_per_block'])
        self.assertEqual(result['counting']['bar_identity'], 'unknown')

    def test_overview_is_not_overwritten(self):
        overview = self.write('overview', packet())
        passage.build(overview, [], self.root / 'out')
        with self.assertRaises(FileExistsError):
            passage.build(overview, [], self.root / 'out')

    def check_captured_phrase(self, wrong_source=False):
        outgoing, incoming = packet(), packet()
        outgoing['source'], incoming['source'] = '/music/A.wav', '/music/B.wav'
        deck = {'playing': True, 'position_seconds': 0, 'beatgrid_valid': True,
                'preparation_pending': False, 'is_master': True, 'sync_enabled': False,
                'grid_revision': 7, 'beatgrid_uniform': True, 'actual_playback_bpm': 120}
        state = {'engine': {'load_revisions': [1, 2], 'decks': [
            dict(deck, track={'path': '/music/A.wav'}),
            dict(deck, playing=False, is_master=False, sync_enabled=True,
                 track={'path': '/music/B.wav'}),
        ]}}
        if wrong_source:
            incoming['source'] = '/music/unrelated.wav'
        files = [self.write(name, {'structuredContent': value}) for name, value in
                 [('state', state), ('a', outgoing), ('b', incoming)]]
        args = ['phrase_check', '--state', str(files[0]), '--outgoing-packet', str(files[1]),
                '--incoming-packet', str(files[2]), '--outgoing', 'A', '--incoming', 'B',
                '--launch', '2', '--handover', '10', '--entry', '0', '--arrival', '8']
        output = io.StringIO()
        with patch('sys.argv', args), contextlib.redirect_stdout(output):
            status = phrase.main()
        return status, json.loads(output.getvalue())

    def test_phrase_capture_requires_matching_source_even_without_load_revision(self):
        status, result = self.check_captured_phrase(wrong_source=True)
        self.assertEqual(status, 2)
        self.assertFalse(result['timing_pass'])
        self.assertIn('source differs from deck B', result['errors'][0])

    def test_phrase_capture_accepts_matching_source_and_grid(self):
        status, result = self.check_captured_phrase()
        self.assertEqual(status, 0, result)
        self.assertTrue(result['timing_pass'])
        self.assertEqual(result['scope'], 'clock_coordinates_only')


if __name__ == '__main__':
    unittest.main()
