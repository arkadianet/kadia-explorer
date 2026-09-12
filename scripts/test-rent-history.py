#!/usr/bin/env python3
"""Chain negative controls plus synthetic algorithm tests (not consensus certification)."""
import contextlib
import copy
import hashlib
import io
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import rent_history as r

ROOT = Path(__file__).resolve().parents[1]
RULE = {'from_height': 0, 'to_height': 2**32-1, 'period': r.PERIOD,
        'arithmetic': 'wrapping_i32', 'storage_fee_factor': 1_250_000,
        'pointer_types': ['Short'], 'validator_revision': 'synthetic-test-only',
        'parameter_source': 'synthetic-test-only'}
MANIFEST = {'rules': [RULE]}
OWNER = '0008cd02' + '11'*32
PAYOUT = '0008cd03' + '22'*32


def box(value, tree=OWNER, height=0, tx='11'*32, index=0):
    b = {'value': value, 'ergoTree': tree, 'creationHeight': height, 'assets': [],
         'additionalRegisters': {}, 'transactionId': tx, 'index': index}
    b['boxId'] = hashlib.blake2b(r.box_bytes(b), digest_size=32).hexdigest()
    return b


def inp(b, proof='', selector='0300'):
    return {'boxId': b['boxId'], 'spendingProof': {'proofBytes': proof,
            'extension': {} if selector is None else {'127': selector}}}


def tx(tid, ins, specs):
    return {'id': tid, 'inputs': ins, 'outputs': [box(v, tree, r.PERIOD, tid, n) for n, (v, tree) in enumerate(specs)]}


def block(txs):
    return {'header': {'id': 'ff'*32, 'height': r.PERIOD, 'timestamp': 1000},
            'blockTransactions': {'transactions': txs}}


class NegativeControl(unittest.TestCase):
    def test_confirmed_recent_blocks_emission_and_fee_collection(self):
        # Expectations are pinned chain transactions, not derived classifier positives.
        blocks = [json.loads((ROOT / f'tests/fixtures/blocks/{h}.json').read_text()) for h in (1866000, 1866001, 1866002)]
        boxes = {o['boxId']: o for b in blocks for t in b['blockTransactions']['transactions'] for o in t['outputs']}
        controls = {
            1866001: ('756e5fa7131c167b9fe8daacffe5918d2f0d7acb3e2ab0b98d0e01720fb04159', '2f01db61e329f94106f1fc08301bcc4cc1c15dcab5f76708e6d623117e339990'),
            1866002: ('f57fcdea7f7df7f8a02518bdfafa7b17feb3850dc1fd48ca7eab2d6c298c5edc', '4d0b8f7f70e564ac49da56dfed1700f812a553545be106cda3e0fbca59cc3b46')}
        emission_tree = blocks[0]['blockTransactions']['transactions'][0]['outputs'][0]['ergoTree']
        for b in blocks[1:]:
            result = r.classify_block(b, boxes, {}, MANIFEST)
            self.assertFalse(any(t['verified_claim'] for t in result['transactions']))
            self.assertTrue(all(t['classification_complete'] for t in result['transactions']))
            by_id = {t['id']: t for t in b['blockTransactions']['transactions']}
            outcomes = {t['txid']: t for t in result['transactions']}
            for tid, tree in zip(controls[b['header']['height']], (emission_tree, r.FEE_TREE)):
                self.assertIn(tid, by_id)
                for i, outcome in zip(by_id[tid]['inputs'], outcomes[tid]['inputs']):
                    self.assertEqual(i['spendingProof']['proofBytes'], '')
                    self.assertEqual(boxes[i['boxId']]['ergoTree'], tree)
                    self.assertIn(b['header']['height']-boxes[i['boxId']]['creationHeight'], (1, 2))
                    self.assertEqual(outcome['reason'], 'below_maturity')

    def test_fee_tree_matches_repository_pin(self):
        self.assertIn('"' + r.FEE_TREE + '"', (ROOT / 'crates/xp-store/src/apply.rs').read_text())


class Predicate(unittest.TestCase):
    def test_boundaries_proof_and_selectors(self):
        b = box(1_000_000_000)
        output = box(950_000_000, height=r.PERIOD)
        for age, outcome in ((r.PERIOD-1, 'ordinary_spend'), (r.PERIOD, 'verified_rent'), (r.PERIOD+1, 'verified_rent')):
            o = dict(output, creationHeight=age)
            self.assertEqual(r.classify_input(inp(b), b, [o], age, RULE)['outcome'], outcome)
        self.assertEqual(r.classify_input(inp(b, proof='ab'), b, [output], r.PERIOD, RULE)['reason'], 'nonempty_proof')
        self.assertEqual(r.classify_input(inp(b, selector=None), b, [output], r.PERIOD, RULE)['reason'], 'no_rent_selector')
        self.assertEqual(r.classify_input({'boxId': b['boxId']}, b, [output], r.PERIOD, RULE)['outcome'], 'unresolved')
        for selector in ('0400', '0302', '03', '0301'):
            self.assertNotEqual(r.classify_input(inp(b, selector=selector), b, [output], r.PERIOD, RULE)['outcome'], 'verified_rent')
        self.assertEqual(r.classify_input(inp(b), b, [output], r.PERIOD, None)['reason'], 'unsupported_parameter_history')

    def test_consumed_pointer_and_recreation_integrity(self):
        b = box(1_000_000)
        self.assertEqual(r.classify_input(inp(b), b, [box(1_000_000, PAYOUT)], r.PERIOD, RULE)['shape'], 'fully_consumed')
        self.assertEqual(r.classify_input(inp(b), b, [], r.PERIOD, RULE)['outcome'], 'ordinary_spend')
        rich = box(1_000_000_000)
        for output in (box(950_000_000, PAYOUT, r.PERIOD), box(1, OWNER, r.PERIOD), box(950_000_000, OWNER, r.PERIOD-1)):
            with self.assertRaisesRegex(ValueError, 'contradicts'):
                r.classify_input(inp(rich), rich, [output], r.PERIOD, RULE)
        bad = dict(rich, value=123)
        with self.assertRaisesRegex(ValueError, 'mismatch'):
            r.classify_input(inp(rich), bad, [], r.PERIOD, RULE)

    def test_wrapping_negative_charge_requires_top_up(self):
        b = box(10_000_000_000, '00'*2000)
        q = (len(r.box_bytes(b))*1_250_000+2**31) % 2**32-2**31
        self.assertLess(q, 0)
        o = dict(b, value=b['value']-q, creationHeight=r.PERIOD)
        self.assertEqual(r.classify_input(inp(b), b, [o], r.PERIOD, RULE)['shape'], 'recreated')


class Accounting(unittest.TestCase):
    def fixture(self, mixed=False, second=False):
        old = box(1_000_000_000)
        parent = tx('22'*32, [inp(old)], [(950_000_000, OWNER), (50_000_000, '0008d3')])
        parents, boxes = [parent], {old['boxId']: old}
        child_inputs = [inp(parent['outputs'][1])]
        value = 50_000_000
        if mixed:
            funder = box(50_000_000, PAYOUT, r.PERIOD-1)
            boxes[funder['boxId']] = funder
            child_inputs.append(inp(funder, proof='ab'))
            value += 50_000_000
        if second:
            other = box(1_000_000_000, OWNER, tx='44'*32)
            boxes[other['boxId']] = other
            p2 = tx('55'*32, [inp(other)], [(950_000_000, OWNER), (50_000_000, '0008d3')])
            parents.append(p2)
            child_inputs.append(inp(p2['outputs'][1]))
            value += 50_000_000
        child = tx('33'*32, child_inputs, [(value-10_000_000, PAYOUT), (10_000_000, r.FEE_TREE)])
        return parents, child, boxes

    def test_cpfp_once_mode_and_payout(self):
        parents, child, boxes = self.fixture()
        # Fee-collection edges must not join the component or count fees a second time.
        collect = tx('66'*32, [inp(child['outputs'][1])], [(10_000_000, PAYOUT)])
        result = r.classify_block(block(parents+[child, collect]), boxes, {}, MANIFEST)
        claim, family = result['transactions'][0], result['families'][0]
        self.assertEqual(claim['rent_nano'], '50000000')
        self.assertEqual(claim['parent_fee_nano'], '0')
        self.assertEqual(claim['mode'], 'bot_fee_paying')
        self.assertEqual(family['family_fee_nano'], '10000000')
        self.assertEqual(claim['net_proceeds_nano'], '40000000')
        self.assertNotIn(collect['id'], family['members'])
        self.assertEqual(family['payouts'][0]['p2pk'], PAYOUT[6:])

    def test_mixed_and_multiple_claims_are_null(self):
        for mixed, second in ((True, False), (False, True)):
            parents, child, boxes = self.fixture(mixed, second)
            result = r.classify_block(block(parents+[child]), boxes, {}, MANIFEST)
            self.assertEqual(len(result['families']), 1)
            self.assertEqual(result['families'][0]['family_fee_nano'], '10000000')
            self.assertIsNone(result['families'][0]['net_proceeds_nano'])
            for row in result['transactions'][:len(parents)]:
                self.assertIsNone(row['allocated_fee_nano'])
                self.assertIsNotNone(row['allocation_reason'])

    def test_self_claim_and_limit(self):
        parents, _, boxes = self.fixture()
        row = r.classify_block(block(parents), boxes, {}, MANIFEST)['transactions'][0]
        self.assertEqual(row['mode'], 'miner_self_claim')
        parents, child, boxes = self.fixture()
        result = r.classify_block(block(parents+[child]), boxes, {}, MANIFEST, max_edges=0)
        self.assertIsNone(result['families'][0]['family_fee_nano'])
        self.assertEqual(result['transactions'][0]['mode'], 'fee_free_parent')

    def test_shared_pointer_counted_once_and_voluntary_return(self):
        a, b = box(1_000_000), box(1_000_000, tx='22'*32)
        parent = tx('33'*32, [inp(a), inp(b)], [(500_000, OWNER), (1_500_000, PAYOUT)])
        row = r.classify_block(block([parent]), {a['boxId']: a, b['boxId']: b}, {}, MANIFEST)['transactions'][0]
        self.assertEqual(row['rent_nano'], '1500000')
        self.assertEqual([i['shape'] for i in row['inputs']], ['fully_consumed']*2)

    def test_generic_absorbers_do_not_identify_or_join_unconnected_claims(self):
        parents, _, boxes = self.fixture()
        b = box(1_000_000, tx='88'*32)
        boxes[b['boxId']] = b
        parents.append(tx('99'*32, [inp(b)], [(1_000_000, '0008d3')]))
        result = r.classify_block(block(parents), boxes, {}, MANIFEST)
        self.assertEqual(len(result['families']), 2)
        self.assertTrue(all(f['payouts'][0]['p2pk'] is None for f in result['families']))


class Runner(unittest.TestCase):
    def candidate_file(self, path, old, parent):
        summary = {'type': 'summary', 'coverage_complete': True, 'scan_complete': True,
                   'candidate_spends': 1, 'distinct_heights': 1, 'distinct_spending_transactions': 1,
                   'indexed_tip': r.PERIOD, 'indexed_tip_id': 'ff'*32}
        records = [{'type': 'candidate_spend', 'box_id': old['boxId'], 'spending_tx_id': parent['id'], 'spend_height': r.PERIOD},
                   {'type': 'candidate_transaction', 'spending_tx_id': parent['id'], 'spend_height': r.PERIOD}, summary]
        path.write_text(''.join(json.dumps(x)+'\n' for x in records))
        return summary

    def test_resume_replays_complete_jsonl_and_binds_rules(self):
        old = box(1_000_000)
        parent = tx('22'*32, [inp(old)], [(1_000_000, PAYOUT)])
        evidence = {'block': block([parent]), 'boxes': {old['boxId']: old}, 'missing_boxes': {}}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.candidate_file(root/'candidates', old, parent)
            (root/'rules').write_text(json.dumps(MANIFEST))
            (root/'config').write_text('[source]\nurl = "http://fixture.invalid"\n')
            args = SimpleNamespace(candidates=root/'candidates', rules=root/'rules', config=root/'config', work=root/'work',
                                   from_height=0, to_height=2**32-1, concurrency=1)
            results = []
            with patch.object(r.Node, 'anchor', return_value='ff'*32), patch.object(r.Node, 'evidence', return_value=evidence) as fetch:
                for _ in range(2):
                    output = io.StringIO()
                    with contextlib.redirect_stdout(output), contextlib.redirect_stderr(io.StringIO()):
                        r.run(args)
                    results.append(output.getvalue())
                self.assertEqual(fetch.call_count, 1)
                self.assertEqual(results[0], results[1])
                (root/'rules').write_text('{"rules": []}')
                with self.assertRaisesRegex(ValueError, 'manifest changed'):
                    r.run(args)
            with patch.object(r.Node, 'anchor', return_value='ee'*32):
                with self.assertRaisesRegex(ValueError, 'no longer canonical'):
                    r.run(args)

    def test_incomplete_candidates_and_pointer_overflow(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'candidates'
            path.write_text('{"type":"start"}\n')
            with self.assertRaisesRegex(ValueError, 'incomplete'):
                r.read_candidates(path)
        for pointer in ('03808004', '030000', '04808080808000'):
            with self.assertRaises(ValueError):
                r.pointer(pointer, ['Short', 'Int'])

    def test_network_timeout_is_not_infinite_progress_loop(self):
        old = box(1_000_000)
        parent = tx('22'*32, [inp(old)], [(1_000_000, PAYOUT)])
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.candidate_file(root/'candidates', old, parent)
            (root/'rules').write_text(json.dumps(MANIFEST))
            (root/'config').write_text('[source]\nurl = "http://fixture.invalid"\n')
            args = SimpleNamespace(candidates=root/'candidates', rules=root/'rules', config=root/'config', work=root/'work',
                                   from_height=0, to_height=2**32-1, concurrency=1)
            with patch.object(r.Node, 'anchor', return_value='ff'*32), patch.object(r.Node, 'evidence', side_effect=TimeoutError('timeout')):
                with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(TimeoutError):
                    r.run(args)


class Reconciliation(unittest.TestCase):
    def test_equal_counts_different_ids_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'census').write_text(json.dumps({'txid': 'a', 'height': 1846000})+'\n')
            row = {'txid': 'b', 'height': 1846000, 'candidate': True, 'verified_claim': True, 'inputs': []}
            (root/'ours').write_text(json.dumps({'type': 'block', 'height': 1846000, 'transactions': [row]})+'\n')
            stream = io.StringIO()
            with contextlib.redirect_stdout(stream), patch.object(r, 'read_candidates', return_value=({}, {})):
                status = r.reconcile(SimpleNamespace(census=root/'census', classification=root/'ours', candidates=root/'candidates'))
            result = json.loads(stream.getvalue())
            self.assertEqual(status, 1)
            self.assertEqual(result['verified_set']['only_ours'], ['b'])
            self.assertEqual(result['verified_set']['only_census'], ['a'])


if __name__ == '__main__':
    unittest.main()
