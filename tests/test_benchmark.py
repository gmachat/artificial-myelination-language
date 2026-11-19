import json
import unittest

from aml.benchmark import benchmark_results, run_benchmarks


class BenchmarkTests(unittest.TestCase):
    def test_corpus_has_positive_aggregate_source_reduction(self):
        results = benchmark_results()
        for comparison in results["comparisons"].values():
            totals = comparison["totals"]
            self.assertGreater(totals["comparison_tokens"], totals["aml_tokens"])
            self.assertGreater(totals["source_reduction_percent"], 0)
        self.assertEqual(results["scope"], "authored source only")

    def test_json_output_is_machine_readable(self):
        output = json.loads(run_benchmarks(as_json=True))
        self.assertEqual(len(output["comparisons"]["python"]["cases"]), 3)
        self.assertEqual(len(output["comparisons"]["javascript"]["cases"]), 3)
        self.assertEqual(len(output["comparisons"]["ruby"]["cases"]), 3)
        self.assertEqual(len(output["comparisons"]["go"]["cases"]), 3)
        self.assertIn(output["tokenizer"], {"cl100k_base", "lexical_estimate"})

    def test_can_select_javascript_only(self):
        output = benchmark_results(language="javascript")
        self.assertEqual(set(output["comparisons"]), {"javascript"})


if __name__ == "__main__":
    unittest.main()
