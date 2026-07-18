import unittest
from unittest.mock import patch

import chess

from tools import parallel_bot


def enc(uci):
    move = chess.Move.from_uci(uci)
    return move.from_square * 64 + move.to_square


class ParallelBotDecisionTests(unittest.TestCase):
    def test_empty_worker_passes_fall_back_to_one_worker_engine(self):
        first = enc("e2e4")
        second = enc("d2d4")

        def passes(_hist, move, _ceiling):
            return {} if move == first else {1: parallel_bot.Fraction(1, 2)}

        with patch.object(parallel_bot, "root_moves", return_value=[first, second]), \
                patch.object(parallel_bot, "child_passes", side_effect=passes), \
                patch.object(parallel_bot, "sequential_move", return_value=second) as fallback:
            move, depth = parallel_bot.parallel_move([], ceiling=3, workers=2)
        self.assertEqual((move, depth), (second, 0))
        fallback.assert_called_once_with([], 3)

    def test_all_children_participate_in_common_depth_argmax(self):
        first = enc("e2e4")
        second = enc("d2d4")
        values = {
            first: {1: parallel_bot.Fraction(3, 4), 2: parallel_bot.Fraction(2, 3)},
            second: {1: parallel_bot.Fraction(1, 2)},
        }
        with patch.object(parallel_bot, "root_moves", return_value=[first, second]), \
                patch.object(parallel_bot, "child_passes",
                             side_effect=lambda _h, move, _c: values[move]), \
                patch.object(parallel_bot, "sequential_move") as fallback:
            move, depth = parallel_bot.parallel_move([], ceiling=3, workers=2)
        # Child values are complemented. At the only common depth, second wins.
        self.assertEqual((move, depth), (second, 2))
        fallback.assert_not_called()

    def test_only_explicitly_common_completed_depths_are_used(self):
        first = enc("e2e4")
        second = enc("d2d4")
        values = {
            first: {1: parallel_bot.Fraction(3, 4),
                    3: parallel_bot.Fraction(1, 4)},
            second: {1: parallel_bot.Fraction(1, 2),
                     2: parallel_bot.Fraction(1, 8),
                     3: parallel_bot.Fraction(3, 4)},
        }
        with patch.object(parallel_bot, "root_moves", return_value=[first, second]), \
                patch.object(parallel_bot, "child_passes",
                             side_effect=lambda _h, move, _c: values[move]), \
                patch.object(parallel_bot, "sequential_move") as fallback:
            move, depth = parallel_bot.parallel_move([], ceiling=4, workers=2)
        # Depth 2 is absent for the first child and must never enter deepening.
        self.assertEqual((move, depth), (first, 4))
        fallback.assert_not_called()

    def test_all_closed_orbits_choose_first_generated_move(self):
        first = enc("e2e4")
        second = enc("d2d4")
        with patch.object(parallel_bot, "root_moves", return_value=[first, second]), \
                patch.object(parallel_bot.chess.Board, "_transposition_key",
                             return_value=("same",)):
            move, depth = parallel_bot.parallel_move([], ceiling=3, workers=2)
        self.assertEqual((move, depth), (first, 0))


if __name__ == "__main__":
    unittest.main()
