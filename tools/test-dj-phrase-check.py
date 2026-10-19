#!/usr/bin/env python3
"""Musical timeline regressions: event counts, bar identity and overlap strategy."""
import copy
import importlib.util
from pathlib import Path
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / '.agents/skills/djing-art/scripts/phrase_check.py'
spec = importlib.util.spec_from_file_location('phrase_check', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def packet(events, boundaries):
    return {'metadata': {'neural_rhythm': {'beats_per_bar': 4}, 'cue_points': [
                {'name': 'Human marker', 'start_seconds': s, 'origin': 'manual', 'confirmed': True}
                for s in boundaries]},
            'beat_events': {'truncated': False, 'events': [
                {'seconds': s, 'beat_position': n, 'downbeat': n % 4 == 0} for s, n in events]}}


def state():
    a = {'playing': True, 'position_seconds': 100, 'beatgrid_valid': True,
         'preparation_pending': False, 'is_master': True, 'sync_enabled': False,
         'beatgrid_uniform': False, 'actual_playback_bpm': 124}
    b = dict(a, playing=False, position_seconds=0, is_master=False, sync_enabled=True)
    return {'engine': {'decks': [a, b], 'load_revisions': [1, 1], 'engine_epoch': 123}}


class PhraseCheck(unittest.TestCase):
    def test_paused_preflight_keeps_coordinate_and_sync_checks_without_claiming_execution(self):
        a = packet([(120, 240), (152, 304)], [])
        b = packet([(0, 0), (32, 64)], [])
        s = state(); s['engine']['decks'][0]['playing'] = False
        args = (a, b, s, 0, 1, 120, 152, 0, 32)
        self.assertFalse(module.check(*args)['timing_pass'])
        r = module.check(*args, preflight=True)
        self.assertTrue(r['timing_pass'], r)
        self.assertFalse(r['execution_ready'])
        s['engine']['decks'][1]['sync_enabled'] = False
        self.assertFalse(module.check(*args, preflight=True)['timing_pass'])
        s['engine']['decks'][0]['playing'] = True
        self.assertFalse(module.check(*args, preflight=True)['timing_pass'])

    def check_plan(self, a, b, times, s=None, technique='blend'):
        return module.check(a, b, s or state(), 0, 1, *times, technique)

    def test_true_count_and_bar_relation_pass_even_with_unequal_source_seconds(self):
        a = packet([(346.267303, 704), (377.74, 768)], [346.267303, 377.74])
        b = packet([(15.739364, 32), (46.223789, 96)], [46.223789])
        r = self.check_plan(a, b, [346.267303, 377.74, 15.739364, 46.223789])
        self.assertTrue(r['timing_pass'], r)
        self.assertEqual(r['incoming']['lead_beats'], 64)
        self.assertEqual(r['audible_overlap'], 'not_evaluated')

    def test_66_beat_entry_rejects_64_beat_outgoing_window(self):
        a = packet([(346.267303, 704), (377.74, 768)], [377.74])
        b = packet([(14.758048, 30), (46.223789, 96)], [46.223789])
        r = self.check_plan(a, b, [346.267303, 377.74, 14.758048, 46.223789])
        self.assertFalse(r['timing_pass'])
        self.assertTrue(any('Unequal leads' in e for e in r['errors']))
        self.assertTrue(any('parts of the bar' in e for e in r['errors']))

    def test_equal_mean_duration_does_not_hide_98_vs_96(self):
        a = packet([(339.763184, 702), (386.381622, 800)], [386.381622])
        b = packet([(46.223789, 96), (93.44175, 192)], [46.223789, 93.44175])
        self.assertFalse(self.check_plan(a, b, [339.763184, 386.381622, 46.223789, 93.44175])['timing_pass'])

    def test_inspected_beat_is_not_automatically_a_downbeat_or_phrase(self):
        a = packet([(311.333, 633)], [])
        b = packet([(173.169, 352)], [173.169])
        r = self.check_plan(a, b, [311.333, 311.333, 173.169, 173.169])
        self.assertFalse(r['timing_pass'])
        self.assertTrue(any('downbeats' in e for e in r['errors']))

    def test_unequal_tempo_fade_fails_but_true_phrase_cut_can_pass(self):
        a = packet([(206.802, 448)], [206.802])
        b = packet([(108.473, 224)], [108.473])
        s = state(); s['engine']['decks'][1]['sync_enabled'] = False
        s['engine']['decks'][0]['actual_playback_bpm'] = 130
        self.assertFalse(self.check_plan(a, b, [206.802, 206.802, 108.473, 108.473], s)['timing_pass'])
        self.assertTrue(self.check_plan(a, b, [206.802, 206.802, 108.473, 108.473], s, 'cut')['timing_pass'])

    def test_truncation_rejected_and_passed_clock_cannot_be_reused(self):
        a = packet([(120, 240)], [120]); b = packet([(0, 0)], [0])
        truncated = copy.deepcopy(a); truncated['beat_events']['truncated'] = True
        with self.assertRaisesRegex(ValueError, 'truncated'):
            self.check_plan(truncated, b, [120, 120, 0, 0])
        s = state(); s['engine']['decks'][0]['position_seconds'] = 121
        self.assertFalse(self.check_plan(a, b, [120, 120, 0, 0], s)['timing_pass'])

    def test_unreviewed_pulse_cannot_pass(self):
        a = packet([(120, 240)], [120]); b = packet([(0, 0)], [0])
        b['beat_events']['local_tempo'] = {'pulse_review_required': True}
        self.assertFalse(self.check_plan(a, b, [120, 120, 0, 0])['timing_pass'])

    def test_missing_manual_cues_do_not_block_a_coordinate_check_or_certify_phrases(self):
        a = packet([(120, 240), (152, 304)], [])
        b = packet([(0, 0), (32, 64)], [])
        r = self.check_plan(a, b, [120, 152, 0, 32])
        self.assertTrue(r['timing_pass'], r)
        self.assertEqual(r['anchor_evidence']['incoming_arrival'], [])
        self.assertIn('not proof of a phrase', r['musical_review_required'])


if __name__ == '__main__':
    unittest.main()
