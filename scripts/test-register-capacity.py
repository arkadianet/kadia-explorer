"""Offline collector arithmetic tests; no live collection is claimed."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    "capacity", Path(__file__).with_name("check-register-capacity.py")
)
capacity = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capacity)


class CapacityTests(unittest.TestCase):
    def test_threshold_and_large_counts(self):
        for entries, ceiling, alert in [(79, 100, False), (80, 100, True),
                                        (81, 100, True), (0, 0, True),
                                        (2**64 - 1, 2**64 - 1, True)]:
            self.assertEqual(capacity.sample({"entries": str(entries), "ceiling": str(ceiling)})
                             ["alert_80_percent"], alert)

    def test_unconfigured_is_explicitly_unbounded(self):
        for entries in [0, 100_000_001, 2**64 - 1]:
            self.assertEqual(capacity.sample({"entries": str(entries), "ceiling": None}),
                             {"entries": str(entries), "ceiling": None,
                              "alert_80_percent": None, "unbounded": True})

    def test_invalid_sample(self):
        for entries in [-1, 2**64, "invalid"]:
            with self.assertRaises(ValueError):
                capacity.sample({"entries": entries, "ceiling": "100"})


if __name__ == "__main__":
    unittest.main()
