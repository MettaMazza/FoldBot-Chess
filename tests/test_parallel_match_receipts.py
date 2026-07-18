import unittest

import chess

from tools.verify_parallel_match_receipts import expected_result


class ParallelMatchReceiptTests(unittest.TestCase):
    def test_replayed_checkmate_is_attributed_to_bot_colour(self):
        board = chess.Board()
        for text in ("f2f3", "e7e5", "g2g4", "d8h4"):
            board.push_uci(text)
        self.assertEqual(expected_result(board, bot_white=False), "win")
        self.assertEqual(expected_result(board, bot_white=True), "loss")

    def test_nonterminal_bound_is_recorded_as_draw_cap(self):
        self.assertEqual(expected_result(chess.Board(), bot_white=False), "draw(cap)")


if __name__ == "__main__":
    unittest.main()
