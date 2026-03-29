from __future__ import annotations

import json
import re
from typing import Any

from .errors import HarnessError
from .process import run
from .troubleshooting import classify_gh_failure

ISSUE_RELATIONSHIPS_QUERY = """
query($owner: String!, $repo: String!, $number: Int!) {
  repository(owner: $owner, name: $repo) {
    issue(number: $number) {
      id
      number
      title
      url
      state
      parent {
        id
        number
        title
        state
      }
      blockedBy(first: 100) {
        nodes {
          id
          number
          title
          state
        }
      }
      blocking(first: 100) {
        nodes {
          id
          number
          title
          state
        }
      }
      subIssues(first: 100) {
        nodes {
          id
          number
          title
          state
        }
      }
    }
  }
}
"""

ISSUE_REFERENCE_FRAGMENT = """
fragment IssueRef on Issue {
  id
  number
  title
  url
  state
}
"""

ISSUE_DETAILS_QUERY = (
    ISSUE_REFERENCE_FRAGMENT
    + """
query($owner: String!, $repo: String!, $number: Int!) {
  repository(owner: $owner, name: $repo) {
    issue(number: $number) {
      ...IssueRef
      body
      parent {
        ...IssueRef
      }
      blockedBy(first: 100) {
        nodes {
          ...IssueRef
        }
      }
      blocking(first: 100) {
        nodes {
          ...IssueRef
        }
      }
      subIssues(first: 100) {
        nodes {
          ...IssueRef
        }
      }
      projectItems(first: 20) {
        nodes {
          id
          project {
            id
            title
          }
          status: fieldValueByName(name: "Status") {
            __typename
            ... on ProjectV2ItemFieldSingleSelectValue {
              name
              optionId
            }
          }
          priority: fieldValueByName(name: "Priority") {
            __typename
            ... on ProjectV2ItemFieldSingleSelectValue {
              name
              optionId
            }
          }
          size: fieldValueByName(name: "Size") {
            __typename
            ... on ProjectV2ItemFieldSingleSelectValue {
              name
              optionId
            }
          }
          kind: fieldValueByName(name: "kind") {
            __typename
            ... on ProjectV2ItemFieldSingleSelectValue {
              name
              optionId
            }
          }
          kindLegacy: fieldValueByName(name: "Kind") {
            __typename
            ... on ProjectV2ItemFieldSingleSelectValue {
              name
              optionId
            }
          }
        }
      }
    }
  }
}
"""
)

ISSUE_COMMENTS_QUERY = (
    ISSUE_REFERENCE_FRAGMENT
    + """
query(
  $owner: String!
  $repo: String!
  $number: Int!
  $commentsFirst: Int!
  $commentsAfter: String
) {
  repository(owner: $owner, name: $repo) {
    issue(number: $number) {
      ...IssueRef
      comments(first: $commentsFirst, after: $commentsAfter) {
        nodes {
          id
          bodyText
          createdAt
          updatedAt
          url
          author {
            login
          }
        }
        pageInfo {
          hasNextPage
          endCursor
        }
      }
    }
  }
}
"""
)

REPOSITORY_ISSUES_QUERY = (
    ISSUE_REFERENCE_FRAGMENT
    + """
query($owner: String!, $repo: String!, $first: Int!, $after: String) {
  repository(owner: $owner, name: $repo) {
    issues(first: $first, after: $after, orderBy: {field: CREATED_AT, direction: DESC}) {
      nodes {
        ...IssueRef
        body
        parent {
          ...IssueRef
        }
        blockedBy(first: 100) {
          nodes {
            ...IssueRef
          }
        }
        blocking(first: 100) {
          nodes {
            ...IssueRef
          }
        }
        subIssues(first: 100) {
          nodes {
            ...IssueRef
          }
        }
        projectItems(first: 20) {
          nodes {
            id
            project {
              id
              title
            }
            status: fieldValueByName(name: "Status") {
              __typename
              ... on ProjectV2ItemFieldSingleSelectValue {
                name
                optionId
              }
            }
            priority: fieldValueByName(name: "Priority") {
              __typename
              ... on ProjectV2ItemFieldSingleSelectValue {
                name
                optionId
              }
            }
            size: fieldValueByName(name: "Size") {
              __typename
              ... on ProjectV2ItemFieldSingleSelectValue {
                name
                optionId
              }
            }
            kind: fieldValueByName(name: "kind") {
              __typename
              ... on ProjectV2ItemFieldSingleSelectValue {
                name
                optionId
              }
            }
            kindLegacy: fieldValueByName(name: "Kind") {
              __typename
              ... on ProjectV2ItemFieldSingleSelectValue {
                name
                optionId
              }
            }
          }
        }
      }
      pageInfo {
        hasNextPage
        endCursor
      }
    }
  }
}
"""
)

PROJECT_KIND_CONTEXT_QUERY = """
query($owner: String!, $repo: String!) {
  repository(owner: $owner, name: $repo) {
    id
    name
    owner {
      id
      login
    }
    projectsV2(first: 20) {
      nodes {
        id
        title
        closed
        fields(first: 50) {
          nodes {
            __typename
            ... on ProjectV2SingleSelectField {
              id
              name
              options {
                id
                name
                color
                description
              }
            }
          }
        }
      }
    }
  }
}
"""

PULL_REQUEST_REVIEWS_QUERY = """
query($owner: String!, $repo: String!, $number: Int!) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $number) {
      number
      url
      title
      headRefName
      headRefOid
      reviews(first: 100) {
        nodes {
          id
          fullDatabaseId
          state
          bodyText
          submittedAt
          author {
            login
          }
          commit {
            oid
          }
          comments(first: 100) {
            nodes {
              id
              fullDatabaseId
              bodyText
              path
              line
              originalLine
              outdated
              publishedAt
              author {
                login
              }
              commit {
                oid
              }
            }
          }
        }
      }
    }
  }
}
"""

_CLOSING_REFERENCE_PATTERN = re.compile(
    r"(?im)\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\s*#(?P<number>\d+)\b"
)
_REQUIRED_PROJECT_SINGLE_SELECT_FIELDS = (
    {
        "name": "Status",
        "options": (
            {"name": "Backlog", "color": "GRAY", "description": ""},
            {"name": "In Progress", "color": "BLUE", "description": ""},
        ),
    },
    {
        "name": "kind",
        "options": (
            {"name": "epic", "color": "BLUE", "description": ""},
            {"name": "implementation", "color": "GREEN", "description": ""},
        ),
    },
    {
        "name": "Priority",
        "options": (
            {"name": "P0", "color": "RED", "description": ""},
            {"name": "P1", "color": "ORANGE", "description": ""},
            {"name": "P2", "color": "YELLOW", "description": ""},
        ),
    },
    {
        "name": "Size",
        "options": (
            {"name": "XS", "color": "BLUE", "description": ""},
            {"name": "S", "color": "GREEN", "description": ""},
            {"name": "M", "color": "YELLOW", "description": ""},
            {"name": "L", "color": "ORANGE", "description": ""},
            {"name": "XL", "color": "RED", "description": ""},
        ),
    },
)
_SINGLE_SELECT_COLOR_PATTERN = re.compile(r"^[A-Z_]+$")


def split_repo_name_with_owner(repo_name_with_owner: str) -> tuple[str, str]:
    try:
        owner, repo = repo_name_with_owner.split("/", maxsplit=1)
    except ValueError as exc:
        raise HarnessError(f"Invalid repository name: {repo_name_with_owner}") from exc
    return owner, repo


def run_gh(args: list[str]) -> str:
    return run(["gh", *args])


def run_gh_json(args: list[str]) -> Any:
    output = run_gh(args)
    if not output.strip():
        return None
    try:
        return json.loads(output)
    except json.JSONDecodeError as exc:
        raise HarnessError("GitHub CLI returned invalid JSON.") from exc


def _format_graphql_variable(value: Any) -> tuple[str, str]:
    if isinstance(value, str):
        return "-f", value
    if isinstance(value, bool):
        return "-F", "true" if value else "false"
    if value is None:
        return "-F", "null"
    return "-F", str(value)


def build_graphql_args(query: str, **variables: Any) -> list[str]:
    args = ["api", "graphql", "-f", f"query={query}"]
    for key, value in variables.items():
        flag, rendered = _format_graphql_variable(value)
        args.extend([flag, f"{key}={rendered}"])
    return args


def graphql(query: str, **variables: Any) -> dict[str, Any]:
    payload = run_gh_json(build_graphql_args(query, **variables))
    if not isinstance(payload, dict):
        raise HarnessError("GitHub GraphQL response was empty.")

    errors = payload.get("errors") or []
    if errors:
        messages = []
        for error in errors:
            if isinstance(error, dict):
                messages.append(str(error.get("message", "GraphQL request failed.")))
            else:
                messages.append(str(error))
        raise HarnessError(classify_gh_failure(" ".join(messages)))

    data = payload.get("data")
    if not isinstance(data, dict):
        raise HarnessError("GitHub GraphQL response did not contain data.")
    return data


def get_repo_view(repo_name_with_owner: str) -> dict[str, Any]:
    payload = run_gh_json(
        ["repo", "view", repo_name_with_owner, "--json", "nameWithOwner,defaultBranchRef"]
    )
    if not isinstance(payload, dict):
        raise HarnessError("Could not resolve repository metadata.")
    return payload


def get_issue_relationships(repo_name_with_owner: str, issue_number: int) -> dict[str, Any]:
    owner, repo = split_repo_name_with_owner(repo_name_with_owner)
    data = graphql(ISSUE_RELATIONSHIPS_QUERY, owner=owner, repo=repo, number=issue_number)
    issue = ((data.get("repository") or {}).get("issue")) or None
    if not issue:
        raise HarnessError(f"Issue #{issue_number} was not found in {repo_name_with_owner}.")
    return issue


def get_issue_node(repo_name_with_owner: str, issue_number: int) -> dict[str, Any]:
    issue = get_issue_relationships(repo_name_with_owner, issue_number)
    return {"id": issue["id"], "number": issue["number"], "title": issue["title"]}


def get_issue_details(repo_name_with_owner: str, issue_number: int) -> dict[str, Any]:
    owner, repo = split_repo_name_with_owner(repo_name_with_owner)
    data = graphql(ISSUE_DETAILS_QUERY, owner=owner, repo=repo, number=issue_number)
    issue = ((data.get("repository") or {}).get("issue")) or None
    if not issue:
        raise HarnessError(f"Issue #{issue_number} was not found in {repo_name_with_owner}.")
    return issue


def get_issue_comments(repo_name_with_owner: str, issue_number: int) -> dict[str, Any]:
    owner, repo = split_repo_name_with_owner(repo_name_with_owner)
    comments: list[dict[str, Any]] = []
    issue_metadata: dict[str, Any] | None = None
    comments_after: str | None = None

    while True:
        data = graphql(
            ISSUE_COMMENTS_QUERY,
            owner=owner,
            repo=repo,
            number=issue_number,
            commentsFirst=100,
            commentsAfter=comments_after,
        )
        issue = ((data.get("repository") or {}).get("issue")) or None
        if not issue:
            raise HarnessError(f"Issue #{issue_number} was not found in {repo_name_with_owner}.")

        if issue_metadata is None:
            issue_metadata = {
                "number": issue.get("number"),
                "title": issue.get("title"),
                "url": issue.get("url"),
                "state": issue.get("state"),
            }

        connection = issue.get("comments") or {}
        comments.extend(connection.get("nodes") or [])

        page_info = connection.get("pageInfo") or {}
        if not page_info.get("hasNextPage"):
            break
        comments_after = page_info.get("endCursor")
        if not comments_after:
            break

    return {**(issue_metadata or {}), "comments": comments}


def get_repository_project_bootstrap_context(repo_name_with_owner: str) -> dict[str, Any]:
    owner, repo = split_repo_name_with_owner(repo_name_with_owner)
    data = graphql(PROJECT_KIND_CONTEXT_QUERY, owner=owner, repo=repo)
    repository = (data.get("repository") or {}) if isinstance(data, dict) else {}
    repository_id = repository.get("id")
    if not repository_id:
        raise HarnessError(f"Could not resolve repository metadata for {repo_name_with_owner}.")
    return repository


def get_repository_kind_field_context(repo_name_with_owner: str, requested_kind: str) -> dict[str, str]:
    repository = get_repository_project_bootstrap_context(repo_name_with_owner)
    repository_id = repository.get("id")

    matches: list[tuple[dict[str, Any], dict[str, Any]]] = []
    for project in ((repository.get("projectsV2") or {}).get("nodes") or []):
        kind_field = _get_project_single_select_field(project, "kind")
        if kind_field is not None:
            matches.append((project, kind_field))

    if not matches:
        raise HarnessError(
            "No repository project with a `kind` field was found. "
            "Bootstrap the project first and ensure the custom `kind` field exists."
        )

    open_matches = [(project, field) for project, field in matches if not project.get("closed")]
    if open_matches:
        matches = open_matches

    if len(matches) != 1:
        titles = ", ".join(sorted((project.get("title") or "Untitled") for project, _ in matches))
        raise HarnessError(f"Expected exactly one repository project with a `kind` field, found: {titles}")

    project, kind_field = matches[0]
    option = _match_single_select_option(kind_field.get("options") or [], requested_kind, field_name="kind")
    project_id = project.get("id")
    kind_field_id = kind_field.get("id")
    kind_option_id = option.get("id")

    if not all([project_id, kind_field_id, kind_option_id]):
        raise HarnessError("Repository project `kind` metadata is incomplete; cannot create a managed issue.")

    return {
        "repository_id": repository_id,
        "project_id": project_id,
        "kind_field_id": kind_field_id,
        "kind_option_id": kind_option_id,
    }


def bootstrap_repository_project(repo_name_with_owner: str) -> dict[str, str]:
    repository = get_repository_project_bootstrap_context(repo_name_with_owner)
    repository_id = repository.get("id")
    owner_id = ((repository.get("owner") or {}).get("id")) or None
    if not repository_id or not owner_id:
        raise HarnessError(f"Could not resolve repository metadata for {repo_name_with_owner}.")

    project = _select_bootstrap_project(repository)
    if project is None:
        _, repo = split_repo_name_with_owner(repo_name_with_owner)
        project = create_repository_project(owner_id, repository_id, repo)

    project_id = project.get("id")
    if not project_id:
        raise HarnessError("Repository project metadata is incomplete; cannot bootstrap the project.")

    fields_by_name: dict[str, dict[str, Any]] = {}
    for field_spec in _REQUIRED_PROJECT_SINGLE_SELECT_FIELDS:
        field_name = str(field_spec["name"])
        required_options = list(field_spec["options"])
        field = _get_project_single_select_field(project, field_name)
        if field is None:
            field = create_project_single_select_field(project_id, field_name, required_options)
        else:
            field = _ensure_required_single_select_options(field, required_options)
        fields_by_name[normalize_status_label(field_name)] = field

    kind_field = fields_by_name["kind"]

    kind_field_id = kind_field.get("id")
    if not kind_field_id:
        raise HarnessError("Repository project `kind` metadata is incomplete after bootstrap.")

    return {
        "repository_id": repository_id,
        "project_id": project_id,
        "kind_field_id": kind_field_id,
    }


def get_pull_request_reviews(repo_name_with_owner: str, pr_number: int) -> dict[str, Any]:
    owner, repo = split_repo_name_with_owner(repo_name_with_owner)
    data = graphql(PULL_REQUEST_REVIEWS_QUERY, owner=owner, repo=repo, number=pr_number)
    pull_request = ((data.get("repository") or {}).get("pullRequest")) or None
    if not pull_request:
        raise HarnessError(f"Pull request #{pr_number} was not found in {repo_name_with_owner}.")
    return pull_request


def list_repository_issues(repo_name_with_owner: str) -> list[dict[str, Any]]:
    owner, repo = split_repo_name_with_owner(repo_name_with_owner)
    issues: list[dict[str, Any]] = []
    issues_after: str | None = None

    while True:
        data = graphql(
            REPOSITORY_ISSUES_QUERY,
            owner=owner,
            repo=repo,
            first=50,
            after=issues_after,
        )
        connection = ((data.get("repository") or {}).get("issues")) or {}
        issues.extend(connection.get("nodes") or [])

        page_info = connection.get("pageInfo") or {}
        if not page_info.get("hasNextPage"):
            break
        issues_after = page_info.get("endCursor")
        if not issues_after:
            break

    return issues


def add_blocked_by(issue_id: str, blocking_issue_id: str) -> None:
    graphql(
        """
mutation($issueId: ID!, $blockingIssueId: ID!) {
  addBlockedBy(input: {issueId: $issueId, blockingIssueId: $blockingIssueId}) {
    clientMutationId
  }
}
""",
        issueId=issue_id,
        blockingIssueId=blocking_issue_id,
    )


def remove_blocked_by(issue_id: str, blocking_issue_id: str) -> None:
    graphql(
        """
mutation($issueId: ID!, $blockingIssueId: ID!) {
  removeBlockedBy(input: {issueId: $issueId, blockingIssueId: $blockingIssueId}) {
    clientMutationId
  }
}
""",
        issueId=issue_id,
        blockingIssueId=blocking_issue_id,
    )


def add_sub_issue(parent_issue_id: str, sub_issue_id: str, replace_parent: bool = False) -> None:
    graphql(
        """
mutation($issueId: ID!, $subIssueId: ID!, $replaceParent: Boolean!) {
  addSubIssue(
    input: {
      issueId: $issueId
      subIssueId: $subIssueId
      replaceParent: $replaceParent
    }
  ) {
    clientMutationId
  }
}
    """,
        issueId=parent_issue_id,
        subIssueId=sub_issue_id,
        replaceParent=replace_parent,
    )


def remove_sub_issue(parent_issue_id: str, sub_issue_id: str) -> None:
    graphql(
        """
mutation($issueId: ID!, $subIssueId: ID!) {
  removeSubIssue(input: {issueId: $issueId, subIssueId: $subIssueId}) {
    clientMutationId
  }
}
""",
        issueId=parent_issue_id,
        subIssueId=sub_issue_id,
    )


def create_issue(repository_id: str, title: str, body: str) -> dict[str, Any]:
    data = graphql(
        """
mutation($repositoryId: ID!, $title: String!, $body: String!) {
  createIssue(input: {repositoryId: $repositoryId, title: $title, body: $body}) {
    issue {
      id
      number
      title
      url
      body
    }
  }
}
""",
        repositoryId=repository_id,
        title=title,
        body=body,
    )
    created_issue = ((data.get("createIssue") or {}).get("issue")) or None
    if not created_issue:
        raise HarnessError("GitHub did not return the created issue.")
    return created_issue


def update_issue(issue_id: str, *, title: str, body: str) -> None:
    graphql(
        """
mutation($issueId: ID!, $title: String!, $body: String!) {
  updateIssue(input: {id: $issueId, title: $title, body: $body}) {
    issue {
      id
    }
  }
}
""",
        issueId=issue_id,
        title=title,
        body=body,
    )


def add_issue_comment(subject_id: str, body: str) -> None:
    graphql(
        """
mutation($subjectId: ID!, $body: String!) {
  addComment(input: {subjectId: $subjectId, body: $body}) {
    commentEdge {
      node {
        id
      }
    }
  }
}
""",
        subjectId=subject_id,
        body=body,
    )


def add_issue_to_project(project_id: str, issue_id: str) -> str:
    data = graphql(
        """
mutation($projectId: ID!, $contentId: ID!) {
  addProjectV2ItemById(input: {projectId: $projectId, contentId: $contentId}) {
    item {
      id
    }
  }
}
""",
        projectId=project_id,
        contentId=issue_id,
    )
    item = ((data.get("addProjectV2ItemById") or {}).get("item")) or None
    item_id = (item or {}).get("id")
    if not item_id:
        raise HarnessError("GitHub did not return a project item after adding the issue to the project.")
    return item_id


def update_project_item_single_select_value(project_id: str, item_id: str, field_id: str, option_id: str) -> None:
    graphql(
        """
mutation($projectId: ID!, $itemId: ID!, $fieldId: ID!, $optionId: String!) {
  updateProjectV2ItemFieldValue(
    input: {
      projectId: $projectId
      itemId: $itemId
      fieldId: $fieldId
      value: {singleSelectOptionId: $optionId}
    }
  ) {
    projectV2Item {
      id
    }
  }
}
""",
        projectId=project_id,
        itemId=item_id,
        fieldId=field_id,
        optionId=option_id,
    )


def create_repository_project(owner_id: str, repository_id: str, title: str) -> dict[str, Any]:
    data = graphql(
        """
mutation($ownerId: ID!, $repositoryId: ID!, $title: String!) {
  createProjectV2(input: {ownerId: $ownerId, repositoryId: $repositoryId, title: $title}) {
    projectV2 {
      id
      title
      closed
      fields(first: 50) {
        nodes {
          __typename
          ... on ProjectV2SingleSelectField {
            id
            name
            options {
              id
              name
              color
              description
            }
          }
        }
      }
    }
  }
}
""",
        ownerId=owner_id,
        repositoryId=repository_id,
        title=title,
    )
    project = ((data.get("createProjectV2") or {}).get("projectV2")) or None
    if not project:
        raise HarnessError("GitHub did not return the created repository project.")
    return project


def create_project_single_select_field(project_id: str, name: str, options: list[dict[str, Any]]) -> dict[str, Any]:
    field = _mutate_project_single_select_field(
        "createProjectV2Field",
        """
mutation($projectId: ID!, $name: String!) {
  createProjectV2Field(
    input: {
      projectId: $projectId
      name: $name
      dataType: SINGLE_SELECT
      singleSelectOptions: %s
    }
  ) {
    projectV2Field {
      __typename
      ... on ProjectV2SingleSelectField {
        id
        name
        options {
          id
          name
          color
          description
        }
      }
    }
  }
}
"""
        % _render_single_select_options_literal(options),
        projectId=project_id,
        name=name,
    )
    if field.get("__typename") != "ProjectV2SingleSelectField":
        raise HarnessError("GitHub did not return a single-select field for the repository project.")
    return field


def update_project_single_select_field(
    field_id: str,
    name: str,
    options: list[dict[str, Any]],
) -> dict[str, Any]:
    field = _mutate_project_single_select_field(
        "updateProjectV2Field",
        """
mutation($fieldId: ID!, $name: String!) {
  updateProjectV2Field(
    input: {
      fieldId: $fieldId
      name: $name
      singleSelectOptions: %s
    }
  ) {
    projectV2Field {
      __typename
      ... on ProjectV2SingleSelectField {
        id
        name
        options {
          id
          name
          color
          description
        }
      }
    }
  }
}
"""
        % _render_single_select_options_literal(options),
        fieldId=field_id,
        name=name,
    )
    if field.get("__typename") != "ProjectV2SingleSelectField":
        raise HarnessError("GitHub did not return the updated single-select field.")
    return field


def build_closing_body(description: str, issue_number: int) -> str:
    closing_reference = f"Closes #{issue_number}"
    if any(int(match.group("number")) == issue_number for match in _CLOSING_REFERENCE_PATTERN.finditer(description)):
        return description.strip()
    if not description.strip():
        return closing_reference
    return f"{description.rstrip()}\n\n{closing_reference}"


def normalize_status_label(value: str) -> str:
    return re.sub(r"[-_\s]+", "-", value.strip().lower())


def _match_single_select_option(options: list[dict[str, Any]], requested_name: str, *, field_name: str) -> dict[str, Any]:
    exact = next((option for option in options if option.get("name") == requested_name), None)
    if exact is not None:
        return exact

    normalized_requested = normalize_status_label(requested_name)
    normalized_matches = [
        option for option in options if normalize_status_label(str(option.get("name") or "")) == normalized_requested
    ]

    if len(normalized_matches) == 1:
        return normalized_matches[0]

    available = ", ".join(sorted(str(option.get("name") or "") for option in options if option.get("name")))
    if not normalized_matches:
        raise HarnessError(f"Unknown {field_name} option '{requested_name}'. Available values: {available}")

    raise HarnessError(f"{field_name} option '{requested_name}' matches multiple values.")


def _project_titles(projects: list[dict[str, Any]]) -> str:
    return ", ".join(sorted((project.get("title") or "Untitled") for project in projects))


def _get_project_single_select_field(project: dict[str, Any], name: str) -> dict[str, Any] | None:
    matches = [
        field
        for field in ((project.get("fields") or {}).get("nodes") or [])
        if field.get("__typename") == "ProjectV2SingleSelectField"
        and normalize_status_label(str(field.get("name") or "")) == normalize_status_label(name)
    ]
    if not matches:
        return None
    if len(matches) > 1:
        title = project.get("title") or "Untitled"
        raise HarnessError(f"Repository project '{title}' has multiple '{name}' single-select fields.")
    return matches[0]


def _select_bootstrap_project(repository: dict[str, Any]) -> dict[str, Any] | None:
    projects = list(((repository.get("projectsV2") or {}).get("nodes") or []))
    open_projects = [project for project in projects if not project.get("closed")]
    if not open_projects:
        return None

    open_with_kind = [
        project for project in open_projects if _get_project_single_select_field(project, "kind") is not None
    ]
    if len(open_with_kind) == 1:
        return open_with_kind[0]
    if len(open_with_kind) > 1:
        raise HarnessError(
            f"Expected exactly one open repository project with a `kind` field, found: {_project_titles(open_with_kind)}"
        )

    if len(open_projects) == 1:
        return open_projects[0]

    raise HarnessError(
        f"Expected exactly one open repository project to bootstrap, found: {_project_titles(open_projects)}"
    )


def _ensure_required_single_select_options(
    field: dict[str, Any],
    required_options: list[dict[str, Any]],
) -> dict[str, Any]:
    options = [_coerce_single_select_option(option) for option in (field.get("options") or [])]
    normalized_existing = {normalize_status_label(option["name"]) for option in options}
    missing = [
        _coerce_single_select_option(option)
        for option in required_options
        if normalize_status_label(str(option.get("name") or "")) not in normalized_existing
    ]
    if not missing:
        return field

    updated_options = [*options, *missing]
    return update_project_single_select_field(
        str(field.get("id") or ""),
        str(field.get("name") or ""),
        updated_options,
    )


def _coerce_single_select_option(option: dict[str, Any]) -> dict[str, str]:
    name = str(option.get("name") or "").strip()
    if not name:
        raise HarnessError("Repository project field metadata is incomplete; found an unnamed single-select option.")

    color = str(option.get("color") or "GRAY").strip().upper()
    if not _SINGLE_SELECT_COLOR_PATTERN.match(color):
        color = "GRAY"

    return {
        "name": name,
        "color": color,
        "description": str(option.get("description") or "").strip(),
    }


def _render_single_select_options_literal(options: list[dict[str, Any]]) -> str:
    rendered = ", ".join(_render_single_select_option_literal(option) for option in options)
    return f"[{rendered}]"


def _render_single_select_option_literal(option: dict[str, Any]) -> str:
    normalized = _coerce_single_select_option(option)
    return (
        "{"
        f'name: {json.dumps(normalized["name"])}, '
        f'color: {normalized["color"]}, '
        f'description: {json.dumps(normalized["description"])}'
        "}"
    )


def _mutate_project_single_select_field(mutation_name: str, query: str, **variables: Any) -> dict[str, Any]:
    data = graphql(query, **variables)
    field = ((data.get(mutation_name) or {}).get("projectV2Field")) or None
    if not field:
        raise HarnessError("GitHub did not return repository project field metadata.")
    return field
