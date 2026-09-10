#!/usr/bin/env python3
"""Read-only HTTP export for the ignored history_http_snapshot_replay route test.
Usage: python3 scripts/capture-history-fixture.py /tmp/history-mainnet.json
No database access. Reject an export if the indexed tip changes during capture.
"""
import json
import sys
import urllib.request

BASE = 'http://127.0.0.1:18091'
ADDRESSES = [
    '9gD9khJaxi3SvcX9VVPQ3vnV3xUTonVQe3Fvg5X7cGGbXMRgd8i',
    '9iKFBBrryPhBYVGDKHuZQW7SuLfuTdUJtTPzecbQ5pQQzD4VykC',
    '9i51m3reWk99iw8WF6PgxbUT6ZFKhzJ1PmD11vEuGu125hRaKAH',
]


def get(path):
    with urllib.request.urlopen(BASE + path, timeout=60) as response:
        return json.load(response)


def pages(path):
    result = []
    cursor = None
    seen = set()
    while True:
        page = get(path + '&limit=500&dir=asc' + (f'&cursor={cursor}' if cursor else ''))
        result.extend(page['items'])
        cursor = page.get('next_cursor')
        if cursor is None:
            return result
        assert cursor not in seen, 'cursor cycle'
        seen.add(cursor)


tip = get('/v1/status')['indexed']
result = {'tip': tip, 'addresses': []}
for address in ADDRESSES:
    root = '/v1/addresses/' + address
    summary = get(root)
    # Full history for both reported addresses; a third current-UTXO comparison.
    full = address in ADDRESSES[:2]
    boxes = pages(root + '/boxes?unspent=' + ('false' if full else 'true'))
    live = pages(root + '/boxes?unspent=true')
    if full:
        heights = {tx['id']: tx['height'] for tx in pages(root + '/txs?')}
    else:
        heights = {b['tx_id']: get('/v1/txs/' + b['tx_id'])['height'] for b in boxes}
    for box in boxes:
        box['inclusion_height'] = heights[box['tx_id']]
    result['addresses'].append({'address': address, 'full': full, 'summary': summary,
                                'boxes': boxes, 'live': [b['id'] for b in live]})
    print(address, len(boxes), 'candidates;', len(live), 'unspent', flush=True)
assert get('/v1/status')['indexed'] == tip, 'tip moved; repeat export'
with open(sys.argv[1], 'w') as output:
    json.dump(result, output)
print('Saved', sys.argv[1], 'at', tip)
