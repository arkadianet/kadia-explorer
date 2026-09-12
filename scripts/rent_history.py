#!/usr/bin/env python3
"""Phase 2a: confirmed rent classification and JSONL audit; Python 3.11+, stdlib only."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import sys
import time
import tomllib
import urllib.error
import urllib.request

PERIOD = 1_051_200
VERSION = 1
FEE_TREE = '1005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798ea02d192a39a8cc7a701730073011001020402d19683030193a38cc7b2a57300000193c2b2a57301007473027303830108cdeeac93b1a57304'
MAX_BYTES = 16 * 1024 * 1024
MAX_EDGES = 10_000


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':')).encode()


def digest(value):
    return hashlib.sha256(encoded(value)).hexdigest()


def vlq(value):
    if not isinstance(value, int) or not 0 <= value <= 2**64 - 1:
        raise ValueError('invalid unsigned integer')
    result = bytearray()
    while value >= 128:
        result.append((value & 127) | 128)
        value >>= 7
    return bytes(result + bytes([value]))


def box_bytes(box):
    """Verbatim Sigma box encoding, authenticated against the confirmed input ID."""
    regs = box['additionalRegisters']
    if set(regs) != {f'R{i+4}' for i in range(len(regs))} or len(regs) > 6:
        raise ValueError('noncontiguous registers')
    if not 0 <= box['creationHeight'] < 2**32 or not 0 <= box['index'] < 2**16:
        raise ValueError('box integer out of range')
    txid = bytes.fromhex(box['transactionId'])
    if len(txid) != 32 or not 0 < box['value'] < 2**63:
        raise ValueError('invalid box reference/value')
    data = vlq(box['value']) + bytes.fromhex(box['ergoTree']) + vlq(box['creationHeight'])
    data += vlq(len(box['assets']))
    for token in box['assets']:
        token_id = bytes.fromhex(token['tokenId'])
        if len(token_id) != 32:
            raise ValueError('invalid token id')
        data += token_id + vlq(token['amount'])
    data += bytes([len(regs)]) + b''.join(bytes.fromhex(regs[f'R{i+4}']) for i in range(len(regs)))
    return data + txid + vlq(box['index'])


def authenticate(box, box_id):
    data = box_bytes(box)
    if box['boxId'] != box_id or hashlib.blake2b(data, digest_size=32).hexdigest() != box_id:
        raise ValueError('box ID/serialization mismatch')
    return data


def pointer(raw, accepted):
    data = bytes.fromhex(raw)
    if not data or data[0] not in (3, 4):
        raise ValueError('unsupported selector type')
    kind = {3: 'Short', 4: 'Int'}[data[0]]
    if kind not in accepted:
        raise ValueError('selector type not accepted by pinned rule')
    value = 0
    for pos, byte in enumerate(data[1:]):
        if pos >= (3 if kind == 'Short' else 5):
            raise ValueError('selector overflow')
        value |= (byte & 127) << (7 * pos)
        if byte < 128:
            if pos + 2 != len(data) or value >= 2**(16 if kind == 'Short' else 32):
                raise ValueError('invalid selector encoding')
            return (value >> 1) ^ -(value & 1)
    raise ValueError('truncated selector')


def rule_at(manifest, height):
    found = [r for r in manifest['rules'] if r['from_height'] <= height <= r['to_height']]
    if len(found) > 1:
        raise ValueError('overlapping rule intervals')
    if not found:
        return None
    rule = found[0]
    if (rule['arithmetic'] != 'wrapping_i32' or rule['period'] != PERIOD
            or not rule['validator_revision'] or not rule['parameter_source']
            or rule['pointer_types'] not in (['Short'], ['Short', 'Int'])
            or not -(2**31) <= rule['storage_fee_factor'] < 2**31):
        raise ValueError('unsupported/incomplete rule manifest')
    return rule


def classify_input(inp, box, outputs, height, rule):
    result = {'box_id': inp['boxId'], 'outcome': 'ordinary_spend', 'reason': 'below_maturity',
              'pointer_constant': None, 'output_index': None}
    proof = inp.get('spendingProof', {})
    if box is not None:
        data = authenticate(box, inp['boxId'])
        result.update(creation_height=box['creationHeight'], age=height-box['creationHeight'],
                      serialized_size=len(data))
        if height - box['creationHeight'] < PERIOD:
            return result
    # A present nonempty proof or absent selector excludes this path even if box lookup failed.
    if isinstance(proof.get('proofBytes'), str) and proof['proofBytes']:
        bytes.fromhex(proof['proofBytes'])
        result['reason'] = 'nonempty_proof'
        return result
    if proof.get('proofBytes') == '' and isinstance(proof.get('extension'), dict) and '127' not in proof['extension']:
        result['reason'] = 'no_rent_selector'
        return result
    result.update(outcome='unresolved', reason='missing_input_box')
    if box is None:
        return result
    if proof.get('proofBytes') != '' or not isinstance(proof.get('extension'), dict):
        result['reason'] = 'missing_proof_or_extension'
        return result
    result['pointer_constant'] = proof['extension'].get('127')
    if rule is None:
        result['reason'] = 'unsupported_parameter_history'
        return result
    try:
        index = pointer(result['pointer_constant'], rule['pointer_types'])
    except (ValueError, TypeError):
        result['reason'] = 'selector_decode_or_rule_unsupported'
        return result
    result['output_index'] = index
    if not 0 <= index < len(outputs):
        result.update(outcome='ordinary_spend', reason='selector_out_of_range_fallback')
        return result
    charge = (len(data) * rule['storage_fee_factor'] + 2**31) % 2**32 - 2**31
    result['protocol_charge_nano'] = str(charge)
    target = outputs[index]
    if box['value'] - charge <= 0:
        result.update(outcome='verified_rent', reason='accepted_branch', shape='fully_consumed')
    elif (target['creationHeight'] == height and target['value'] >= box['value'] - charge
          and all(target[k] == box[k] for k in ('ergoTree', 'assets', 'additionalRegisters'))):
        result.update(outcome='verified_rent', reason='accepted_branch', shape='recreated')
    else:
        # Pinned branch returns false (not an exception/fallback) on this path.
        raise ValueError('confirmed block contradicts pinned rent branch')
    return result


def fee(tx):
    return sum(o['value'] for o in tx['outputs'] if o['ergoTree'] == FEE_TREE)


def classify_block(block, boxes, candidates, manifest, max_edges=MAX_EDGES):
    height = block['header']['height']
    txs = block['blockTransactions']['transactions']
    ids = [t['id'] for t in txs]
    if len(set(ids)) != len(ids) or not set(candidates) <= set(ids):
        raise ValueError('duplicate transaction or phase-1 transaction missing from canonical block')
    rule = rule_at(manifest, height)
    resolved = dict(boxes)
    rows, inputs_by_tx, created, edges = {}, {}, {}, []
    for tx in txs:
        inputs = [resolved.get(i['boxId']) for i in tx['inputs']]
        inputs_by_tx[tx['id']] = inputs
        outcomes = [classify_input(i, b, tx['outputs'], height, rule) for i, b in zip(tx['inputs'], inputs)]
        verified = [b for b, r in zip(inputs, outcomes) if r['outcome'] == 'verified_rent']
        mature = any(b is not None and height-b['creationHeight'] >= PERIOD for b in inputs)
        if tx['id'] in candidates:
            actual = {i['boxId'] for i, b in zip(tx['inputs'], inputs) if b is not None and height-b['creationHeight'] >= PERIOD}
            if any(bid not in {i['boxId'] for i in tx['inputs']} for bid in candidates[tx['id']]):
                raise ValueError('phase-1 candidate box missing from transaction')
            if all(b is not None for b in inputs) and actual != set(candidates[tx['id']]):
                raise ValueError('phase-1 mature input set mismatch')
        complete = all(b is not None for b in inputs)
        owner_trees = {b['ergoTree'] for b in verified}
        all_trees = {b['ergoTree'] for b in inputs if b is not None}
        deltas = {tree: sum(b['value'] for b in verified if b['ergoTree'] == tree)
                  - sum(o['value'] for o in tx['outputs'] if o['ergoTree'] == tree) for tree in owner_trees}
        clean = bool(verified) and complete and len(verified) == len(inputs)
        # Other funding can return to an owner tree; conservative null preserves audit deltas.
        exact = clean and all(v >= 0 for v in deltas.values()) and FEE_TREE not in owner_trees
        row = {'type': 'transaction', 'txid': tx['id'], 'height': height, 'block_id': block['header']['id'],
               'candidate': mature or tx['id'] in candidates, 'verified_claim': bool(verified),
               'inputs': outcomes, 'classification_complete': all(r['outcome'] != 'unresolved' for r in outcomes),
               'input_evidence_complete': complete,
               'census_owner_delta_nano': str(sum(b['value'] for b in inputs)-sum(o['value'] for o in tx['outputs'] if o['ergoTree'] in all_trees)) if complete else None,
               'owner_deltas_nano': {k: str(v) for k, v in sorted(deltas.items())},
               'owner_top_up_nano': str(sum(-v for v in deltas.values() if v < 0)),
               'rent_nano': str(sum(deltas.values())) if exact else None,
               'amount_status': 'exact' if exact else 'ambiguous_flows' if complete else 'missing_input_evidence',
               'parent_fee_nano': str(fee(tx)), 'mode': None, 'family_id': None,
               'allocated_fee_nano': None, 'net_proceeds_nano': None}
        if complete and sum(b['value'] for b in inputs) != sum(o['value'] for o in tx['outputs']):
            raise ValueError('input/output ERG conservation mismatch')
        rows[tx['id']] = row
        for inp in tx['inputs']:
            if inp['boxId'] in created:
                parent, output = created[inp['boxId']]
                powners = {b['ergoTree'] for b in inputs_by_tx[parent] if b is not None}
                if output['ergoTree'] != FEE_TREE and output['ergoTree'] not in powners:
                    edges.append((parent, tx['id'], inp['boxId']))
        for output in tx['outputs']:
            authenticate(output, output['boxId'])
            if output['boxId'] in created:
                raise ValueError('duplicate output')
            resolved[output['boxId']] = output
            created[output['boxId']] = (tx['id'], output)
    oversized = len(edges) > max_edges or len(encoded({'block': block, 'boxes': boxes})) > MAX_BYTES
    adjacency = {tid: set() for tid in ids}
    for parent, child, _ in edges:
        adjacency[parent].add(child)
        adjacency[child].add(parent)
    families, visited = [], set()
    txmap = {t['id']: t for t in txs}
    for tid in ids:
        if not rows[tid]['verified_claim'] or tid in visited:
            continue
        component, pending = set(), [tid]
        while pending:
            current = pending.pop()
            if current in component:
                continue
            component.add(current)
            pending.extend(adjacency[current] - component)
        visited.update(component)
        members = sorted(component)
        claims = [m for m in members if rows[m]['verified_claim']]
        family_edges = [list(e) for e in edges if e[0] in component]
        complete = not oversized and all(rows[m]['input_evidence_complete'] for m in members)
        family_fee = sum(fee(txmap[m]) for m in members)
        owners = {b['ergoTree'] for m in claims for b, r in zip(inputs_by_tx[m], rows[m]['inputs']) if r['outcome'] == 'verified_rent'}
        spent = {i['boxId'] for m in members for i in txmap[m]['inputs']}
        boundary = [o for m in members for o in txmap[m]['outputs'] if o['boxId'] not in spent]
        external = [(m, i['boxId']) for m in members for i in txmap[m]['inputs'] if i['boxId'] not in created or created[i['boxId']][0] not in component]
        rent_ids = {r['box_id'] for m in claims for r in rows[m]['inputs'] if r['outcome'] == 'verified_rent'}
        solely_rent = all(bid in rent_ids for _, bid in external)
        # Several parents or ordinary child funding never receive a guessed allocation.
        exact = complete and solely_rent and len(claims) == 1 and rows[tid]['rent_nano'] is not None
        refunds = sum(o['value'] for m in members if m not in claims for o in txmap[m]['outputs'] if o['boxId'] not in spent and o['ergoTree'] in owners)
        gross = sum(int(rows[m]['rent_nano']) for m in claims) if all(rows[m]['rent_nano'] is not None for m in claims) else None
        if gross is None or gross-refunds-family_fee < 0:
            exact = False
        family_id = digest([block['header']['id'], members])
        reason = 'unresolved_limit' if oversized else 'missing_input_evidence' if not complete else 'multiple_claims' if len(claims) > 1 else 'mixed_funding_or_owner_flows' if not exact else None
        family = {'type': 'family', 'family_id': family_id, 'height': height, 'members': members,
                  'claim_txids': claims, 'edges': family_edges, 'family_basis': 'confirmed_dependency',
                  'cpfp_intent': 'not_determinable', 'family_status': 'complete' if complete else reason,
                  'family_fee_nano': str(family_fee) if complete else None,
                  'gross_rent_nano': str(gross) if gross is not None else None,
                  'additional_owner_refunds_nano': str(refunds), 'allocation_reason': reason,
                  'net_proceeds_nano': str(gross-refunds-family_fee) if exact else None,
                  'payouts': [{'box_id': o['boxId'], 'tree': o['ergoTree'], 'value_nano': str(o['value']),
                              'p2pk': o['ergoTree'][6:] if o['ergoTree'].startswith('0008cd') and len(o['ergoTree']) == 72 else None,
                              'attribution_status': 'observed_payout' if o['ergoTree'].startswith('0008cd') and len(o['ergoTree']) == 72 else 'unknown'}
                             for o in boundary if o['ergoTree'] != FEE_TREE and o['ergoTree'] not in owners]}
        families.append(family)
        for claim in claims:
            r = rows[claim]
            r.update(family_id=family_id, mode='bot_fee_paying' if fee(txmap[claim]) or (complete and family_fee) else 'miner_self_claim' if complete else 'fee_free_parent',
                     mode_basis='owner_structural_rule', allocation_reason=reason,
                     allocated_fee_nano=str(family_fee) if exact else None,
                     net_proceeds_nano=family['net_proceeds_nano'] if exact else None)
    return {'type': 'block', 'classifier_version': VERSION, 'height': height,
            'block_id': block['header']['id'], 'timestamp': block['header']['timestamp'],
            'source_assurance': 'trusted_node_response', 'evidence_digest': digest({'block': block, 'boxes': boxes}),
            'rule': rule, 'transactions': list(rows.values()), 'families': families}


def read_candidates(path):
    heights, summary, spends, transactions = {}, None, 0, set()
    with open(path) as source:
        for line in source:
            record = json.loads(line)
            if summary is not None:
                raise ValueError('records after candidate summary')
            if record['type'] == 'candidate_spend':
                h, txid, bid = record['spend_height'], record['spending_tx_id'], record['box_id']
                boxes = heights.setdefault(h, {}).setdefault(txid, set())
                if bid in boxes:
                    raise ValueError('duplicate candidate spend')
                boxes.add(bid)
                spends += 1
            elif record['type'] == 'candidate_transaction':
                key = (record['spend_height'], record['spending_tx_id'])
                if key in transactions:
                    raise ValueError('duplicate candidate transaction')
                transactions.add(key)
            elif record['type'] == 'summary':
                summary = record
    if (not summary or not summary['coverage_complete'] or not summary['scan_complete']
            or summary['candidate_spends'] != spends or summary['distinct_heights'] != len(heights)
            or summary['distinct_spending_transactions'] != len(transactions)
            or transactions != {(h, t) for h, txs in heights.items() for t in txs}):
        raise ValueError('incomplete/inconsistent Phase 1 candidate file')
    return heights, summary


class Node:
    def __init__(self, url):
        self.url = url.rstrip('/')

    def get(self, path):
        with urllib.request.urlopen(self.url + path, timeout=30) as response:
            raw = response.read(MAX_BYTES + 1)
        if len(raw) > MAX_BYTES:
            raise ValueError('node response exceeds evidence limit')
        return json.loads(raw)

    def anchor(self, height):
        headers = self.get(f'/blocks/chainSlice?fromHeight={height}&toHeight={height}')
        matches = [h for h in headers if h['height'] == height]
        if len(matches) != 1:
            raise ValueError('canonical header unavailable or ambiguous')
        return matches[0]['id']

    def evidence(self, height):
        anchor = self.anchor(height)
        block = self.get('/blocks/' + anchor)
        if block['header']['id'] != anchor or block['header']['height'] != height:
            raise ValueError('block/header anchor mismatch')
        boxes, earlier = {}, set()
        missing = {}
        for tx in block['blockTransactions']['transactions']:
            for inp in tx['inputs']:
                bid = inp['boxId']
                if bid not in earlier and bid not in boxes:
                    try:
                        box = self.get('/blockchain/box/byId/' + bid)
                    except urllib.error.HTTPError as error:
                        if error.code not in (404, 501, 503):
                            raise
                        missing[bid] = f'confirmed_box_endpoint_http_{error.code}'
                        continue
                    authenticate(box, bid)
                    if box.get('spentTransactionId') != tx['id'] or not 0 <= box['inclusionHeight'] <= height:
                        raise ValueError('indexed box is not confirmed spent by this transaction')
                    boxes[bid] = box
            earlier.update(o['boxId'] for o in tx['outputs'])
        if self.anchor(height) != anchor:
            raise ValueError('canonical block changed during fetch')
        return {'block': block, 'boxes': boxes, 'missing_boxes': missing, 'source': self.url}


def atomic_json(path, value):
    temporary = path.with_suffix('.pending')
    with open(temporary, 'wb') as output:
        output.write(encoded(value) + b'\n')
        output.flush()
        os.fsync(output.fileno())
    os.replace(temporary, path)


def run(args):
    candidates, summary = read_candidates(args.candidates)
    manifest = json.loads(Path(args.rules).read_text())
    config = tomllib.loads(Path(args.config).read_text())
    node = Node(config['source']['url'])
    if node.anchor(summary['indexed_tip']) != summary['indexed_tip_id']:
        raise ValueError('Phase 1 snapshot is no longer canonical')
    work = Path(args.work)
    work.mkdir(parents=True, exist_ok=True)
    # A manifest binds cached work to source, candidate snapshot, rules and classifier code.
    identity = {'classifier_version': VERSION, 'source': node.url,
                'code_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                'candidate_summary': summary, 'candidate_digest': digest({str(h): {t: sorted(bs) for t, bs in ts.items()} for h, ts in candidates.items()}),
                'rules': manifest}
    identity_path = work / 'manifest.json'
    if identity_path.exists() and json.loads(identity_path.read_text()) != identity:
        raise ValueError('work manifest changed; use a new work directory')
    atomic_json(identity_path, identity)
    heights = sorted(h for h in candidates if args.from_height <= h <= args.to_height)
    start = time.monotonic()

    def process(height):
        evidence_path, result_path = work / f'{height}.evidence.json', work / f'{height}.result.json'
        if evidence_path.exists():
            evidence = json.loads(evidence_path.read_text())
            if node.anchor(height) != evidence['block']['header']['id']:
                raise ValueError(f'reorg at cached height {height}; use new work directory')
        else:
            evidence = node.evidence(height)
            atomic_json(evidence_path, evidence)
        if result_path.exists():
            result = json.loads(result_path.read_text())
            if result['evidence_digest'] != digest({'block': evidence['block'], 'boxes': evidence['boxes']}):
                raise ValueError('cached result/evidence mismatch')
        else:
            result = classify_block(evidence['block'], evidence['boxes'], candidates[height], manifest)
            result['missing_boxes'] = evidence.get('missing_boxes', {})
            # Missing boxes are retried on a later run, never checkpointed as completed work.
            if result['missing_boxes']:
                evidence_path.unlink()
            else:
                atomic_json(result_path, result)
        return result

    print(f'phase2a start heights={len(heights)} concurrency={args.concurrency}', file=sys.stderr, flush=True)
    with ThreadPoolExecutor(max_workers=args.concurrency) as executor:
        # Bounded batches avoid queueing 86,637 futures or retaining all block bodies.
        done = 0
        unresolved = 0
        for offset in range(0, len(heights), args.concurrency):
            futures = [executor.submit(process, h) for h in heights[offset:offset+args.concurrency]]
            for future in futures:
                while True:
                    try:
                        result = future.result(timeout=5)
                        break
                    except TimeoutError:
                        if future.done():
                            raise
                        print(f'phase2a waiting completed={done}/{len(heights)} elapsed_s={time.monotonic()-start:.1f}', file=sys.stderr, flush=True)
                print(json.dumps(result, sort_keys=True, separators=(',', ':')), flush=True)
                unresolved += sum(not r['classification_complete'] for r in result['transactions'])
                done += 1
            elapsed = max(time.monotonic()-start, 0.001)
            print(f'phase2a completed={done}/{len(heights)} blocks_s={done/elapsed:.2f} eta_s={(len(heights)-done)*elapsed/done:.0f} unresolved_transactions={unresolved}', file=sys.stderr, flush=True)
    if node.anchor(summary['indexed_tip']) != summary['indexed_tip_id']:
        raise ValueError('Phase 1 snapshot changed during classification')
    print(json.dumps({'type': 'summary', 'blocks': len(heights), 'candidate_snapshot': summary,
                      'requested_range': [args.from_height, args.to_height],
                      'scan_complete': True, 'unresolved_transactions': unresolved,
                      'classification_complete': unresolved == 0}))


def reconcile(args):
    # The independent export is never generated by this module.
    census = {}
    for line in Path(args.census).read_text().splitlines():
        entry = json.loads(line)
        if entry['txid'] in census:
            raise ValueError('duplicate census txid')
        if not 1846000 <= entry['height'] <= 1864008:
            raise ValueError('census transaction outside independent window')
        census[entry['txid']] = entry
    candidates, candidate_summary = read_candidates(args.candidates)
    expected_heights = {h for h in candidates if 1846000 <= h <= 1864008}
    rows = {}
    seen_heights = set()
    summary = None
    with open(args.classification) as source:
        for line in source:
            block = json.loads(line)
            if block['type'] == 'summary':
                summary = block
                continue
            if block['type'] != 'block' or not 1846000 <= block['height'] <= 1864008:
                continue
            if block['height'] in seen_heights:
                raise ValueError('duplicate classification block')
            seen_heights.add(block['height'])
            rows.update((r['txid'], r) for r in block['transactions'])
    def compare(ours):
        matched, extra, absent = sorted(ours & census.keys()), sorted(ours-census.keys()), sorted(census.keys()-ours)
        amounts = [{'txid': t, 'ours': rows[t]['census_owner_delta_nano'], 'census': str(census[t]['rent_nano'])}
                   for t in matched if 'rent_nano' in census[t] and rows[t]['census_owner_delta_nano'] != str(census[t]['rent_nano'])]
        return {'matched': matched, 'only_ours': extra, 'only_census': absent, 'amount_disagreements': amounts,
                'disagreement_sample': [{'txid': t, 'height': rows[t]['height'] if t in rows else census[t]['height'],
                                         'chain_evidence': rows[t]['inputs'] if t in rows else None,
                                         'reason': 'input_predicate_evidence' if t in rows else 'block_or_transaction_not_in_classification'} for t in (extra+absent)[:20]]}
    candidate = compare({t for t, r in rows.items() if r['candidate']})
    verified = compare({t for t, r in rows.items() if r['verified_claim']})
    complete = bool(seen_heights == expected_heights and summary and summary['candidate_snapshot'] == candidate_summary and summary['scan_complete'] and summary['requested_range'][0] <= 1846000 and summary['requested_range'][1] >= 1864008)
    amounts_available = all('rent_nano' in c for c in census.values())
    passed = complete and amounts_available and len(census) == 5116 and all(not x[k] for x in (candidate, verified) for k in ('only_ours', 'only_census', 'amount_disagreements'))
    result = {'type': 'reconciliation', 'census_count': len(census), 'expected_census_count': 5116,
              'coverage_complete': complete, 'census_amounts_available': amounts_available, 'candidate_set': candidate, 'verified_set': verified,
              'status': 'matched' if passed else 'unreconciled'}
    print(json.dumps(result, indent=2))
    return 0 if passed else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    scan = commands.add_parser('classify')
    for flag in ('candidates', 'config', 'rules', 'work'):
        scan.add_argument('--' + flag, required=True)
    scan.add_argument('--from-height', type=int, default=0)
    scan.add_argument('--to-height', type=int, default=2**32-1)
    scan.add_argument('--concurrency', type=int, choices=range(1, 33), default=12)
    rec = commands.add_parser('reconcile')
    rec.add_argument('--census', required=True)
    rec.add_argument('--candidates', required=True)
    rec.add_argument('--classification', required=True)
    args = parser.parse_args()
    try:
        return reconcile(args) if args.command == 'reconcile' else run(args)
    except (ValueError, KeyError, OSError) as error:
        print(f'rent-history: {error}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
