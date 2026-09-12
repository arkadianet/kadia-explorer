#!/usr/bin/env python3
"""One bounded external capacity sample. Schedule in the operator's collector.

Exit 0 below 80%, 1 at/above 80% or unbounded, 2 on a missed/invalid sample. JSON output is
intended for the collector's timestamped, rotated storage and alert routing.
"""
import argparse
import datetime
import json
import sys
import urllib.request


def sample(data):
    entries = int(data["entries"])
    if not 0 <= entries <= 2**64 - 1:
        raise ValueError("capacity counts outside u64 range")
    if data["ceiling"] is None:
        return {"entries": str(entries), "ceiling": None,
                "alert_80_percent": None, "unbounded": True}
    ceiling = int(data["ceiling"])
    if not 0 <= ceiling <= 2**64 - 1:
        raise ValueError("capacity counts outside u64 range")
    alert = entries * 5 >= ceiling * 4
    return {"entries": str(entries), "ceiling": str(ceiling), "alert_80_percent": alert, "unbounded": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("url", help="Owning explorer's /v1/register-capacity URL")
    args = parser.parse_args()
    result = {"at": datetime.datetime.now(datetime.timezone.utc).isoformat()}
    try:
        with urllib.request.urlopen(args.url, timeout=10) as response:
            raw = response.read(4097)
        if len(raw) > 4096:
            raise ValueError("capacity response exceeds 4096 bytes")
        result.update(sample(json.loads(raw)))
        status = int(result["unbounded"] or result["alert_80_percent"])
    except Exception as error:
        result["sample_error"] = str(error)
        status = 2
    print(json.dumps(result))
    return status


if __name__ == "__main__":
    sys.exit(main())
