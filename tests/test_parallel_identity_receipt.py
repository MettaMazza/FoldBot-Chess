import unittest
from unittest.mock import patch
from pathlib import Path
import json
import tempfile

from tools import verify_parallel_identity as identity


class ParallelIdentityReceiptTests(unittest.TestCase):
    def test_sample_is_generated_systematically_from_engine_order(self):
        mapping = {
            (): [10, 11, 12],
            (10,): [20, 21, 22],
            (11,): [30, 31, 32],
        }
        with patch.object(
                identity.parallel_bot, "root_moves",
                side_effect=lambda hist, _ceiling: mapping[tuple(hist)]):
            histories = identity.generated_histories(2, 2, 3)
        self.assertEqual(histories, [[10, 20], [10, 21], [11, 30], [11, 31]])

    def test_invalid_generation_counts_halt(self):
        with self.assertRaises(ValueError):
            identity.generated_histories(-1, 2, 3)
        with self.assertRaises(ValueError):
            identity.generated_histories(2, 0, 3)

    def test_receipt_verifier_rejects_a_value_disagreement(self):
        with tempfile.TemporaryDirectory() as temporary:
            receipt = Path(temporary) / "receipt.json"
            receipt.write_text(json.dumps({
                "schema": "foldbot-parallel-identity/v1",
                "status": "completed",
                "bindings": {},
                "rows": [{"status": "identical", "sequential_move": 1,
                          "parallel_move": 1, "sequential_value": "1/2",
                          "parallel_value": "2/3"}],
                "identity": {"moves": 1, "values": 1, "disagreements": 0},
            }))
            with self.assertRaisesRegex(RuntimeError, "row disagreement"):
                identity.verify_receipt(receipt)


if __name__ == "__main__":
    unittest.main()
