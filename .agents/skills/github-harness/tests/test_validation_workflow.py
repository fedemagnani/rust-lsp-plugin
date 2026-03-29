from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
SCRIPTS = ROOT / "scripts"

if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

import validate_live_workflow  # noqa: E402


class ValidationWorkflowHelperTests(unittest.TestCase):
    def test_build_validation_names_normalizes_slug_and_titles(self) -> None:
        names = validate_live_workflow.build_validation_names("March 15 Run")

        self.assertEqual(names.slug, "march-15-run")
        self.assertEqual(names.branch, "validation/march-15-run")
        self.assertEqual(names.epic_title, "epic: validation March 15 Run live workflow")
        self.assertEqual(names.implementation_title, "feat: validation March 15 Run implementation path")

    def test_filter_validation_graph_findings_limits_to_tracked_issues(self) -> None:
        report = {
            "hierarchy_cycles": [[10, 11, 10], [90, 91, 90]],
            "dependency_cycles": [[12, 13, 12]],
            "broken_references": [
                {"issue": 10, "relationship": "parent", "target": 99},
                {"issue": 80, "relationship": "parent", "target": 81},
            ],
            "orphaned_children": [
                {"issue": 11, "detail": "Implementation issue has no parent epic."},
                {"issue": 82, "detail": "Implementation issue has no parent epic."},
            ],
        }

        filtered = validate_live_workflow.filter_validation_graph_findings(report, {10, 11, 12})

        self.assertEqual(filtered["hierarchy_cycles"], [[10, 11, 10]])
        self.assertEqual(filtered["dependency_cycles"], [[12, 13, 12]])
        self.assertEqual(len(filtered["broken_references"]), 1)
        self.assertEqual(len(filtered["orphaned_children"]), 1)

    def test_compute_scores_counts_passed_categories_and_isolation(self) -> None:
        steps = [
            {"category": "bootstrap", "status": "passed"},
            {"category": "issue_authoring", "status": "passed"},
            {"category": "issue_read", "status": "failed"},
            {"category": "isolation", "status": "passed"},
            {"category": "pull_request", "status": "skipped"},
        ]

        scores = validate_live_workflow.compute_scores(steps)

        self.assertEqual(scores["effectiveness_soundness"], 75)
        self.assertEqual(scores["feature_completeness"], 22)
        self.assertEqual(scores["isolation"], 100)


if __name__ == "__main__":
    unittest.main()
