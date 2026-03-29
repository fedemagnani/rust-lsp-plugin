from __future__ import annotations

import argparse
import sys
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
SCRIPTS = ROOT / "scripts"

if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

import add_comment  # noqa: E402
import create_project  # noqa: E402
import create_epic_issue  # noqa: E402
import create_implementation_issue  # noqa: E402
import github_harness.authoring as authoring  # noqa: E402
import github_harness.github as github_api  # noqa: E402
from github_harness.errors import HarnessError  # noqa: E402
from github_harness.issues import render_issue_body, rewrite_issue_body  # noqa: E402
import update_issue as update_issue_script  # noqa: E402


def make_option(
    option_id: str,
    name: str,
    *,
    color: str = "GRAY",
    description: str = "",
) -> dict:
    return {
        "id": option_id,
        "name": name,
        "color": color,
        "description": description,
    }


def make_single_select_field(field_id: str, name: str, *, options: list[dict] | None = None) -> dict:
    return {
        "__typename": "ProjectV2SingleSelectField",
        "id": field_id,
        "name": name,
        "options": options or [],
    }


def make_project(
    project_id: str,
    title: str,
    *,
    closed: bool = False,
    fields: list[dict] | None = None,
) -> dict:
    return {
        "id": project_id,
        "title": title,
        "closed": closed,
        "fields": {"nodes": fields or []},
    }


KIND_OPTIONS = [
    {"name": "epic", "color": "BLUE", "description": ""},
    {"name": "implementation", "color": "GREEN", "description": ""},
]

PRIORITY_OPTIONS = [
    {"name": "P0", "color": "RED", "description": ""},
    {"name": "P1", "color": "ORANGE", "description": ""},
    {"name": "P2", "color": "YELLOW", "description": ""},
]

SIZE_OPTIONS = [
    {"name": "XS", "color": "BLUE", "description": ""},
    {"name": "S", "color": "GREEN", "description": ""},
    {"name": "M", "color": "YELLOW", "description": ""},
    {"name": "L", "color": "ORANGE", "description": ""},
    {"name": "XL", "color": "RED", "description": ""},
]

STATUS_OPTIONS = [
    {"name": "Backlog", "color": "GRAY", "description": ""},
    {"name": "In Progress", "color": "BLUE", "description": ""},
]


def make_kind_field(
    field_id: str = "FIELD_KIND",
    *,
    name: str = "kind",
    options: list[dict] | None = None,
) -> dict:
    return make_single_select_field(
        field_id,
        name,
        options=options
        or [
            make_option("OPT_EPIC", "epic", color="BLUE"),
            make_option("OPT_IMPL", "implementation", color="GREEN"),
        ],
    )


def make_status_field(
    field_id: str = "FIELD_STATUS",
    *,
    options: list[dict] | None = None,
) -> dict:
    return make_single_select_field(
        field_id,
        "Status",
        options=options
        or [
            make_option("OPT_BACKLOG", "Backlog", color="GRAY"),
            make_option("OPT_IN_PROGRESS", "In Progress", color="BLUE"),
        ],
    )


def make_priority_field(
    field_id: str = "FIELD_PRIORITY",
    *,
    options: list[dict] | None = None,
) -> dict:
    return make_single_select_field(
        field_id,
        "Priority",
        options=options
        or [
            make_option("OPT_P0", "P0", color="RED"),
            make_option("OPT_P1", "P1", color="ORANGE"),
            make_option("OPT_P2", "P2", color="YELLOW"),
        ],
    )


def make_size_field(field_id: str = "FIELD_SIZE", *, options: list[dict] | None = None) -> dict:
    return make_single_select_field(
        field_id,
        "Size",
        options=options
        or [
            make_option("OPT_XS", "XS", color="BLUE"),
            make_option("OPT_S", "S", color="GREEN"),
            make_option("OPT_M", "M", color="YELLOW"),
            make_option("OPT_L", "L", color="ORANGE"),
            make_option("OPT_XL", "XL", color="RED"),
        ],
    )


class IssueBodyTests(unittest.TestCase):
    def test_render_issue_body_includes_empty_design_heading(self) -> None:
        self.assertEqual(
            render_issue_body(
                description="Current state.",
                contract="Do the thing.",
                acceptance="1. Works.",
                design="",
            ),
            """## Description
Current state.

## Contract
Do the thing.

## Acceptance
1. Works.

## Design notes
""",
        )

    def test_rewrite_issue_body_preserves_untouched_sections(self) -> None:
        body = """## Description
Current state.

## Contract
Do the thing.

## Acceptance
1. Works.

## Design notes
Keep it small.
"""

        rewritten = rewrite_issue_body(body, contract="Do the other thing.")

        self.assertEqual(
            rewritten,
            """## Description
Current state.

## Contract
Do the other thing.

## Acceptance
1. Works.

## Design notes
Keep it small.
""",
        )


class ProjectKindContextTests(unittest.TestCase):
    def test_get_repository_kind_field_context_prefers_the_open_project(self) -> None:
        with mock.patch.object(
            github_api,
            "graphql",
            return_value={
                "repository": {
                    "id": "REPO_1",
                    "projectsV2": {
                        "nodes": [
                            {
                                "id": "PROJECT_CLOSED",
                                "title": "Closed Project",
                                "closed": True,
                                "fields": {
                                    "nodes": [
                                        {
                                            "__typename": "ProjectV2SingleSelectField",
                                            "id": "FIELD_CLOSED",
                                            "name": "kind",
                                            "options": [{"id": "OPT_CLOSED", "name": "implementation"}],
                                        }
                                    ]
                                },
                            },
                            {
                                "id": "PROJECT_OPEN",
                                "title": "Open Project",
                                "closed": False,
                                "fields": {
                                    "nodes": [
                                        {
                                            "__typename": "ProjectV2SingleSelectField",
                                            "id": "FIELD_OPEN",
                                            "name": "kind",
                                            "options": [
                                                {"id": "OPT_EPIC", "name": "epic"},
                                                {"id": "OPT_IMPL", "name": "implementation"},
                                            ],
                                        }
                                    ]
                                },
                            },
                        ]
                    },
                }
            },
        ):
            self.assertEqual(
                github_api.get_repository_kind_field_context("fedemagnani/harness-eng-skills", "implementation"),
                {
                    "repository_id": "REPO_1",
                    "project_id": "PROJECT_OPEN",
                    "kind_field_id": "FIELD_OPEN",
                    "kind_option_id": "OPT_IMPL",
                },
            )

    def test_get_repository_kind_field_context_matches_legacy_kind_name_case_insensitively(self) -> None:
        with mock.patch.object(
            github_api,
            "graphql",
            return_value={
                "repository": {
                    "id": "REPO_1",
                    "projectsV2": {
                        "nodes": [
                            {
                                "id": "PROJECT_OPEN",
                                "title": "Open Project",
                                "closed": False,
                                "fields": {
                                    "nodes": [
                                        {
                                            "__typename": "ProjectV2SingleSelectField",
                                            "id": "FIELD_KIND",
                                            "name": "Kind",
                                            "options": [
                                                {"id": "OPT_EPIC", "name": "epic"},
                                                {"id": "OPT_TASK", "name": "task"},
                                                {"id": "OPT_IMPL", "name": "implementation"},
                                            ],
                                        }
                                    ]
                                },
                            }
                        ]
                    },
                }
            },
        ):
            self.assertEqual(
                github_api.get_repository_kind_field_context("fedemagnani/harness-eng-skills", "implementation"),
                {
                    "repository_id": "REPO_1",
                    "project_id": "PROJECT_OPEN",
                    "kind_field_id": "FIELD_KIND",
                    "kind_option_id": "OPT_IMPL",
                },
            )


class ProjectBootstrapTests(unittest.TestCase):
    def test_bootstrap_repository_project_creates_project_and_required_fields_when_missing(self) -> None:
        repository = {
            "id": "REPO_1",
            "owner": {"id": "OWNER_1"},
            "projectsV2": {"nodes": []},
        }
        created_project = make_project(
            "PROJECT_1",
            "harness-eng-skills",
            fields=[make_status_field()],
        )

        with (
            mock.patch.object(github_api, "get_repository_project_bootstrap_context", return_value=repository),
            mock.patch.object(github_api, "create_repository_project", return_value=created_project) as create_project_mock,
            mock.patch.object(
                github_api,
                "create_project_single_select_field",
                side_effect=[
                    make_kind_field(),
                    make_priority_field(),
                    make_size_field(),
                ],
            ) as create_field_mock,
            mock.patch.object(github_api, "update_project_single_select_field") as update_field_mock,
        ):
            result = github_api.bootstrap_repository_project("fedemagnani/harness-eng-skills")

        self.assertEqual(
            result,
            {
                "repository_id": "REPO_1",
                "project_id": "PROJECT_1",
                "kind_field_id": "FIELD_KIND",
            },
        )
        create_project_mock.assert_called_once_with("OWNER_1", "REPO_1", "harness-eng-skills")
        self.assertEqual(
            create_field_mock.call_args_list,
            [
                mock.call("PROJECT_1", "kind", KIND_OPTIONS),
                mock.call("PROJECT_1", "Priority", PRIORITY_OPTIONS),
                mock.call("PROJECT_1", "Size", SIZE_OPTIONS),
            ],
        )
        update_field_mock.assert_not_called()

    def test_create_repository_project_preserves_returned_builtin_status_field(self) -> None:
        project = make_project(
            "PROJECT_1",
            "Harness",
            fields=[make_status_field()],
        )

        with mock.patch.object(
            github_api,
            "graphql",
            return_value={"createProjectV2": {"projectV2": project}},
        ) as graphql_mock:
            created = github_api.create_repository_project("OWNER_1", "REPO_1", "Harness")

        self.assertEqual(created["fields"]["nodes"][0]["name"], "Status")
        mutation = graphql_mock.call_args.args[0]
        self.assertIn("fields(first: 50)", mutation)
        self.assertIn("... on ProjectV2SingleSelectField", mutation)

    def test_bootstrap_repository_project_validates_existing_matching_project_without_changes(self) -> None:
        repository = {
            "id": "REPO_1",
            "owner": {"id": "OWNER_1"},
            "projectsV2": {
                "nodes": [
                    make_project(
                        "PROJECT_1",
                        "Harness",
                        fields=[
                            make_status_field(),
                            make_kind_field(),
                            make_priority_field(),
                            make_size_field(),
                        ],
                    )
                ]
            },
        }

        with (
            mock.patch.object(github_api, "get_repository_project_bootstrap_context", return_value=repository),
            mock.patch.object(github_api, "create_repository_project") as create_project_mock,
            mock.patch.object(github_api, "create_project_single_select_field") as create_field_mock,
            mock.patch.object(github_api, "update_project_single_select_field") as update_field_mock,
        ):
            result = github_api.bootstrap_repository_project("fedemagnani/harness-eng-skills")

        self.assertEqual(
            result,
            {
                "repository_id": "REPO_1",
                "project_id": "PROJECT_1",
                "kind_field_id": "FIELD_KIND",
            },
        )
        create_project_mock.assert_not_called()
        create_field_mock.assert_not_called()
        update_field_mock.assert_not_called()

    def test_bootstrap_repository_project_repairs_missing_kind_option(self) -> None:
        repository = {
            "id": "REPO_1",
            "owner": {"id": "OWNER_1"},
            "projectsV2": {
                "nodes": [
                    make_project(
                        "PROJECT_1",
                        "Harness",
                        fields=[
                            make_status_field(),
                            make_single_select_field(
                                "FIELD_KIND",
                                "kind",
                                options=[make_option("OPT_EPIC", "epic", color="BLUE")],
                            ),
                            make_priority_field(),
                            make_size_field(),
                        ],
                    )
                ]
            },
        }
        updated_field = make_single_select_field(
            "FIELD_KIND",
            "kind",
            options=[
                make_option("OPT_EPIC", "epic", color="BLUE"),
                make_option("OPT_IMPL", "implementation", color="GREEN"),
            ],
        )

        with (
            mock.patch.object(github_api, "get_repository_project_bootstrap_context", return_value=repository),
            mock.patch.object(github_api, "update_project_single_select_field", return_value=updated_field) as update_field_mock,
        ):
            result = github_api.bootstrap_repository_project("fedemagnani/harness-eng-skills")

        self.assertEqual(
            result,
            {
                "repository_id": "REPO_1",
                "project_id": "PROJECT_1",
                "kind_field_id": "FIELD_KIND",
            },
        )
        update_field_mock.assert_called_once_with(
            "FIELD_KIND",
            "kind",
            KIND_OPTIONS,
        )

    def test_bootstrap_repository_project_preserves_legacy_kind_name_and_options(self) -> None:
        repository = {
            "id": "REPO_1",
            "owner": {"id": "OWNER_1"},
            "projectsV2": {
                "nodes": [
                    make_project(
                        "PROJECT_1",
                        "Harness",
                        fields=[
                            make_status_field(),
                            make_single_select_field(
                                "FIELD_KIND",
                                "Kind",
                                options=[
                                    make_option("OPT_EPIC", "epic", color="BLUE"),
                                    make_option("OPT_TASK", "task", color="GRAY"),
                                ],
                            ),
                            make_priority_field(),
                            make_size_field(),
                        ],
                    )
                ]
            },
        }
        updated_field = make_single_select_field(
            "FIELD_KIND",
            "Kind",
            options=[
                make_option("OPT_EPIC", "epic", color="BLUE"),
                make_option("OPT_TASK", "task", color="GRAY"),
                make_option("OPT_IMPL", "implementation", color="GREEN"),
            ],
        )

        with (
            mock.patch.object(github_api, "get_repository_project_bootstrap_context", return_value=repository),
            mock.patch.object(github_api, "update_project_single_select_field", return_value=updated_field) as update_field_mock,
        ):
            result = github_api.bootstrap_repository_project("fedemagnani/harness-eng-skills")

        self.assertEqual(
            result,
            {
                "repository_id": "REPO_1",
                "project_id": "PROJECT_1",
                "kind_field_id": "FIELD_KIND",
            },
        )
        update_field_mock.assert_called_once_with(
            "FIELD_KIND",
            "Kind",
            [
                {"name": "epic", "color": "BLUE", "description": ""},
                {"name": "task", "color": "GRAY", "description": ""},
                {"name": "implementation", "color": "GREEN", "description": ""},
            ],
        )

    def test_bootstrap_repository_project_rejects_multiple_case_insensitive_kind_fields(self) -> None:
        repository = {
            "id": "REPO_1",
            "owner": {"id": "OWNER_1"},
            "projectsV2": {
                "nodes": [
                    make_project(
                        "PROJECT_1",
                        "Harness",
                        fields=[
                            make_status_field(),
                            make_single_select_field("FIELD_KIND", "kind"),
                            make_single_select_field("FIELD_KIND_CAPS", "Kind"),
                            make_priority_field(),
                            make_size_field(),
                        ],
                    )
                ]
            },
        }

        with mock.patch.object(github_api, "get_repository_project_bootstrap_context", return_value=repository):
            with self.assertRaises(HarnessError):
                github_api.bootstrap_repository_project("fedemagnani/harness-eng-skills")

    def test_bootstrap_repository_project_uses_single_open_project_without_required_fields(self) -> None:
        repository = {
            "id": "REPO_1",
            "owner": {"id": "OWNER_1"},
            "projectsV2": {
                "nodes": [
                    make_project("PROJECT_1", "Bootstrap target"),
                    make_project(
                        "PROJECT_OLD",
                        "Closed project",
                        closed=True,
                        fields=[
                            make_single_select_field(
                                "FIELD_OLD",
                                "kind",
                                options=[make_option("OPT_EPIC", "epic", color="BLUE")],
                            )
                        ],
                    ),
                ]
            },
        }
        with (
            mock.patch.object(github_api, "get_repository_project_bootstrap_context", return_value=repository),
            mock.patch.object(
                github_api,
                "create_project_single_select_field",
                side_effect=[
                    make_status_field(),
                    make_kind_field(),
                    make_priority_field(),
                    make_size_field(),
                ],
            ) as create_field_mock,
        ):
            result = github_api.bootstrap_repository_project("fedemagnani/harness-eng-skills")

        self.assertEqual(result["project_id"], "PROJECT_1")
        self.assertEqual(
            create_field_mock.call_args_list,
            [
                mock.call("PROJECT_1", "Status", STATUS_OPTIONS),
                mock.call("PROJECT_1", "kind", KIND_OPTIONS),
                mock.call("PROJECT_1", "Priority", PRIORITY_OPTIONS),
                mock.call("PROJECT_1", "Size", SIZE_OPTIONS),
            ],
        )

    def test_bootstrap_repository_project_creates_missing_priority_and_size_fields(self) -> None:
        repository = {
            "id": "REPO_1",
            "owner": {"id": "OWNER_1"},
            "projectsV2": {
                "nodes": [
                    make_project(
                        "PROJECT_1",
                        "Harness",
                        fields=[make_status_field(), make_kind_field()],
                    )
                ]
            },
        }

        with (
            mock.patch.object(github_api, "get_repository_project_bootstrap_context", return_value=repository),
            mock.patch.object(
                github_api,
                "create_project_single_select_field",
                side_effect=[make_priority_field(), make_size_field()],
            ) as create_field_mock,
            mock.patch.object(github_api, "update_project_single_select_field") as update_field_mock,
        ):
            result = github_api.bootstrap_repository_project("fedemagnani/harness-eng-skills")

        self.assertEqual(result["project_id"], "PROJECT_1")
        self.assertEqual(
            create_field_mock.call_args_list,
            [
                mock.call("PROJECT_1", "Priority", PRIORITY_OPTIONS),
                mock.call("PROJECT_1", "Size", SIZE_OPTIONS),
            ],
        )
        update_field_mock.assert_not_called()

    def test_bootstrap_repository_project_repairs_priority_and_size_options_without_dropping_extra_values(self) -> None:
        repository = {
            "id": "REPO_1",
            "owner": {"id": "OWNER_1"},
            "projectsV2": {
                "nodes": [
                    make_project(
                        "PROJECT_1",
                        "Harness",
                        fields=[
                            make_status_field(
                                options=[
                                    make_option("OPT_BACKLOG", "Backlog", color="GRAY"),
                                    make_option("OPT_QUEUED", "Queued", color="ORANGE"),
                                ],
                            ),
                            make_kind_field(),
                            make_priority_field(
                                options=[
                                    make_option("OPT_P0", "P0", color="RED"),
                                    make_option("OPT_P3", "P3", color="GRAY"),
                                ],
                            ),
                            make_size_field(
                                options=[
                                    make_option("OPT_XS", "XS", color="BLUE"),
                                    make_option("OPT_XXL", "XXL", color="PURPLE"),
                                ],
                            ),
                        ],
                    )
                ]
            },
        }

        with (
            mock.patch.object(github_api, "get_repository_project_bootstrap_context", return_value=repository),
            mock.patch.object(
                github_api,
                "update_project_single_select_field",
                side_effect=[
                    make_status_field(),
                    make_priority_field(),
                    make_size_field(),
                ],
            ) as update_field_mock,
        ):
            result = github_api.bootstrap_repository_project("fedemagnani/harness-eng-skills")

        self.assertEqual(result["project_id"], "PROJECT_1")
        self.assertEqual(
            update_field_mock.call_args_list,
            [
                mock.call(
                    "FIELD_STATUS",
                    "Status",
                    [
                        {"name": "Backlog", "color": "GRAY", "description": ""},
                        {"name": "Queued", "color": "ORANGE", "description": ""},
                        {"name": "In Progress", "color": "BLUE", "description": ""},
                    ],
                ),
                mock.call(
                    "FIELD_PRIORITY",
                    "Priority",
                    [
                        {"name": "P0", "color": "RED", "description": ""},
                        {"name": "P3", "color": "GRAY", "description": ""},
                        {"name": "P1", "color": "ORANGE", "description": ""},
                        {"name": "P2", "color": "YELLOW", "description": ""},
                    ],
                ),
                mock.call(
                    "FIELD_SIZE",
                    "Size",
                    [
                        {"name": "XS", "color": "BLUE", "description": ""},
                        {"name": "XXL", "color": "PURPLE", "description": ""},
                        {"name": "S", "color": "GREEN", "description": ""},
                        {"name": "M", "color": "YELLOW", "description": ""},
                        {"name": "L", "color": "ORANGE", "description": ""},
                        {"name": "XL", "color": "RED", "description": ""},
                    ],
                ),
            ],
        )

    def test_bootstrap_repository_project_rejects_ambiguous_open_projects(self) -> None:
        repository = {
            "id": "REPO_1",
            "owner": {"id": "OWNER_1"},
            "projectsV2": {
                "nodes": [
                    make_project("PROJECT_1", "First"),
                    make_project("PROJECT_2", "Second"),
                ]
            },
        }

        with mock.patch.object(github_api, "get_repository_project_bootstrap_context", return_value=repository):
            with self.assertRaises(HarnessError):
                github_api.bootstrap_repository_project("fedemagnani/harness-eng-skills")


class ManagedIssueCreationTests(unittest.TestCase):
    def test_create_managed_issue_sets_kind_and_links_relationships(self) -> None:
        with (
            mock.patch.object(
                authoring,
                "get_repository_kind_field_context",
                return_value={
                    "repository_id": "REPO_1",
                    "project_id": "PROJECT_1",
                    "kind_field_id": "FIELD_KIND",
                    "kind_option_id": "OPT_IMPL",
                },
            ),
            mock.patch.object(authoring, "create_issue", return_value={"id": "ISSUE_1", "number": 39}) as create_issue_mock,
            mock.patch.object(authoring, "add_issue_to_project", return_value="ITEM_1"),
            mock.patch.object(authoring, "update_project_item_single_select_value") as update_field_mock,
            mock.patch.object(authoring, "link_issue_relationships") as link_mock,
        ):
            created = authoring.create_managed_issue(
                "fedemagnani/harness-eng-skills",
                kind="implementation",
                title="feat: execution outcomes writer",
                description="Current state.",
                contract="Define the implementation issue.",
                acceptance="1. Works.",
                design="Keep it small.",
                parent=[12],
                blocked_by=[13],
                blocking=[14],
            )

        self.assertEqual(created["id"], "ISSUE_1")
        create_issue_mock.assert_called_once_with(
            "REPO_1",
            "feat: execution outcomes writer",
            """## Description
Current state.

## Contract
Define the implementation issue.

## Acceptance
1. Works.

## Design notes
Keep it small.
""",
        )
        update_field_mock.assert_called_once_with("PROJECT_1", "ITEM_1", "FIELD_KIND", "OPT_IMPL")
        link_mock.assert_called_once_with(
            "fedemagnani/harness-eng-skills",
            issue_id="ISSUE_1",
            parent=[12],
            blocked_by=[13],
            blocking=[14],
        )


class CreateIssueScriptTests(unittest.TestCase):
    def test_create_epic_issue_main_omits_relationship_arguments(self) -> None:
        parser = mock.Mock()
        parser.parse_args.return_value = argparse.Namespace(
            title="epic: execution outcomes",
            description="Current state.",
            contract="Define the epic.",
            acceptance="1. Works.",
            design="Keep it small.",
        )

        with (
            mock.patch.object(create_epic_issue, "build_parser", return_value=parser),
            mock.patch.object(create_epic_issue, "get_repo_name_with_owner", return_value="fedemagnani/harness-eng-skills"),
            mock.patch.object(create_epic_issue, "create_managed_issue") as create_mock,
        ):
            create_epic_issue.main()

        create_mock.assert_called_once_with(
            "fedemagnani/harness-eng-skills",
            kind="epic",
            title="epic: execution outcomes",
            description="Current state.",
            contract="Define the epic.",
            acceptance="1. Works.",
            design="Keep it small.",
            expected_title_type="epic",
        )

    def test_create_implementation_issue_main_parses_relationship_arguments(self) -> None:
        parser = mock.Mock()
        parser.parse_args.return_value = argparse.Namespace(
            title="feat: implement issue authoring",
            description="Current state.",
            contract="Create implementation issues.",
            acceptance="1. Works.",
            design="Keep it small.",
            parent="41",
            blocked_by="42, 43",
            blocking="44",
        )

        with (
            mock.patch.object(create_implementation_issue, "build_parser", return_value=parser),
            mock.patch.object(
                create_implementation_issue,
                "get_repo_name_with_owner",
                return_value="fedemagnani/harness-eng-skills",
            ),
            mock.patch.object(create_implementation_issue, "create_managed_issue") as create_mock,
        ):
            create_implementation_issue.main()

        create_mock.assert_called_once_with(
            "fedemagnani/harness-eng-skills",
            kind="implementation",
            title="feat: implement issue authoring",
            description="Current state.",
            contract="Create implementation issues.",
            acceptance="1. Works.",
            design="Keep it small.",
            parent=[41],
            blocked_by=[42, 43],
            blocking=[44],
        )


class UpdateIssueScriptTests(unittest.TestCase):
    def test_update_issue_main_rewrites_selected_sections(self) -> None:
        parser = mock.Mock()
        parser.parse_args.return_value = argparse.Namespace(
            issue=39,
            title=None,
            description=None,
            contract="Do the other thing.",
            acceptance=None,
            design=None,
        )

        current_body = """## Description
Current state.

## Contract
Do the thing.

## Acceptance
1. Works.

## Design notes
Keep it small.
"""

        with (
            mock.patch.object(update_issue_script, "build_parser", return_value=parser),
            mock.patch.object(
                update_issue_script,
                "get_repo_name_with_owner",
                return_value="fedemagnani/harness-eng-skills",
            ),
            mock.patch.object(
                update_issue_script,
                "get_issue_details",
                return_value={"id": "ISSUE_39", "title": "feat: implement issue authoring", "body": current_body},
            ),
            mock.patch.object(update_issue_script, "update_issue_remote") as update_mock,
        ):
            update_issue_script.main()

        update_mock.assert_called_once_with(
            "ISSUE_39",
            title="feat: implement issue authoring",
            body="""## Description
Current state.

## Contract
Do the other thing.

## Acceptance
1. Works.

## Design notes
Keep it small.
""",
        )


class AddCommentScriptTests(unittest.TestCase):
    def test_add_comment_main_strips_comment_text(self) -> None:
        parser = mock.Mock()
        parser.parse_args.return_value = argparse.Namespace(issue=39, comment="  Need clarification.  ")

        with (
            mock.patch.object(add_comment, "build_parser", return_value=parser),
            mock.patch.object(add_comment, "get_repo_name_with_owner", return_value="fedemagnani/harness-eng-skills"),
            mock.patch.object(add_comment, "get_issue_node", return_value={"id": "ISSUE_39"}),
            mock.patch.object(add_comment, "add_issue_comment") as add_comment_mock,
        ):
            add_comment.main()

        add_comment_mock.assert_called_once_with("ISSUE_39", "Need clarification.")


class CreateProjectScriptTests(unittest.TestCase):
    def test_create_project_main_bootstraps_repository_project(self) -> None:
        parser = mock.Mock()
        parser.parse_args.return_value = argparse.Namespace()

        with (
            mock.patch.object(create_project, "build_parser", return_value=parser),
            mock.patch.object(create_project, "get_repo_name_with_owner", return_value="fedemagnani/harness-eng-skills"),
            mock.patch.object(create_project, "bootstrap_repository_project") as bootstrap_mock,
        ):
            create_project.main()

        bootstrap_mock.assert_called_once_with("fedemagnani/harness-eng-skills")


if __name__ == "__main__":
    unittest.main()
