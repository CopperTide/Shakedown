"""Write the twin's GraphQL schema and the GHES extract its tests read, from GitHub's published GHES
SDL (`schema.docs-enterprise.graphql`).

The twin's schema keeps the object, interface and union types below (with the connections, edges and
mutation payloads over them), every field of theirs whose type it keeps, every argument with its
input types and enums, and no descriptions. It leaves out the fields gh probes to turn on optional
code paths (research D2/§2.3), so gh takes the fewest. The extract is GitHub's own definition of each
type the twin's schema has, whole, for the subset test.

    uv run python scripts/prune_sdl.py <schema.docs-enterprise.graphql> github_twin/graphql/schema.graphql \\
        tests/schemas/ghes-3.20-graphql-subset.graphql
"""
import sys

from graphql import parse, print_ast
from graphql.language import (DocumentNode, EnumTypeDefinitionNode, InputObjectTypeDefinitionNode,
                              InterfaceTypeDefinitionNode, ListTypeNode, NonNullTypeNode, ObjectTypeDefinitionNode,
                              ScalarTypeDefinitionNode, UnionTypeDefinitionNode)

OUTPUTS = {
    "Query", "Mutation", "User", "Bot", "Mannequin", "Team", "Organization", "Repository", "Ref", "PullRequest",
    "Issue", "IssueComment", "PullRequestReview", "ReviewRequest", "PullRequestCommit", "Commit", "GitActor",
    "PullRequestChangedFile", "Label", "Milestone", "StatusCheckRollup", "StatusContext", "CheckRun", "CheckSuite",
    "WorkflowRun", "Workflow", "ReactionGroup", "AutoMergeRequest", "ProjectV2Item", "ProjectV2",
    "ProjectV2ItemFieldSingleSelectValue", "ProjectCard", "Project", "ProjectColumn", "PageInfo", "App",
    "RepositoryOwner", "Actor", "Node", "UniformResourceLocatable", "Closable", "Comment", "Labelable", "Assignable",
    "Lockable", "Reactable", "RepositoryNode", "Updatable", "UpdatableComment", "GitObject", "RequirableByPullRequest",
    "IssueOrPullRequest", "RequestedReviewer", "StatusCheckRollupContext", "SearchResultItem", "Assignee",
    "ProjectV2ItemContent", "ProjectV2ItemFieldValue", "ProjectCardItem", "BranchProtectionRule",
}
ROOT_FIELDS = {
    "Query": {"id", "viewer", "repository", "node", "nodes", "search", "organization", "user", "repositoryOwner"},
    "Mutation": {"createPullRequest", "updatePullRequest", "mergePullRequest", "closePullRequest",
                 "reopenPullRequest", "markPullRequestReadyForReview", "addPullRequestReview", "requestReviews",
                 "addComment", "createIssue", "updateIssue", "closeIssue", "reopenIssue", "addLabelsToLabelable",
                 "removeLabelsFromLabelable"},
}
#: What gh probes for (`__type` queries) to turn on optional code paths: left out.
OPTIONAL = {
    "Issue": {"blockedBy", "blocking", "issueDependenciesSummary"},
    "PullRequest": {"isInMergeQueue", "isMergeQueueEnabled", "mergeQueue", "mergeQueueEntry"},
    "Repository": {"pullRequestTemplates", "autoMergeAllowed", "mergeQueue"},
    "StatusCheckRollupContextConnection": {"checkRunCount", "checkRunCountsByState", "statusContextCount",
                                           "statusContextCountsByState"},
    "WorkflowRun": {"event"},
}
OPTIONAL_VALUES = {"SearchType": {"ISSUE_ADVANCED"}}
KEPT_DIRECTIVES = {"deprecated", "oneOf"}
LEAVES = (ScalarTypeDefinitionNode, EnumTypeDefinitionNode, InputObjectTypeDefinitionNode)
BUILT_IN = {"String", "Int", "Float", "Boolean", "ID"}


def named(t) -> str:
    while isinstance(t, (NonNullTypeNode, ListTypeNode)):
        t = t.type
    return t.name.value


def _field_type(d, field: str) -> str | None:
    for f in d.fields or ():
        if f.name.value == field:
            return named(f.type)
    return None


class Pruner:
    def __init__(self, sdl: str):
        doc = parse(sdl, no_location=True)
        self.defs = {d.name.value: d for d in doc.definitions if hasattr(d, "fields") or hasattr(d, "types")
                     or isinstance(d, LEAVES)}
        self.outputs = set(OUTPUTS)
        self.leaves: set = set()

    def _wanted(self, d) -> bool:
        """A connection or edge over kept nodes, or a kept mutation's payload."""
        name = d.name.value
        if name.endswith("Connection"):
            return _field_type(d, "nodes") in self.outputs or _field_type(d, "edges") in self.outputs
        if name.endswith("Edge"):
            return _field_type(d, "node") in self.outputs
        return False

    def grow(self) -> None:
        payloads = {named(f.type) for f in self.defs["Mutation"].fields if f.name.value in ROOT_FIELDS["Mutation"]}
        self.outputs |= payloads
        while True:
            more = {n for n, d in self.defs.items() if isinstance(d, ObjectTypeDefinitionNode)
                    and n not in self.outputs and self._wanted(d)}
            if not more:
                return
            self.outputs |= more

    def keeps_field(self, type_name: str, f) -> bool:
        if f.name.value in OPTIONAL.get(type_name, ()):
            return False
        if type_name in ROOT_FIELDS and f.name.value not in ROOT_FIELDS[type_name]:
            return False
        t = named(f.type)
        return t in self.outputs or t in BUILT_IN or isinstance(self.defs.get(t), (ScalarTypeDefinitionNode,
                                                                                   EnumTypeDefinitionNode))

    def _need_leaf(self, name: str) -> None:
        if name in self.leaves or not isinstance(self.defs.get(name), LEAVES):
            return
        self.leaves.add(name)
        d = self.defs[name]
        if isinstance(d, InputObjectTypeDefinitionNode):
            for f in d.fields:
                self._need_leaf(named(f.type))

    def _fields(self, name: str, d, from_interfaces: set) -> tuple:
        """The kept fields; one an interface has is not deprecated (GHES deprecates `Project.id`,
        which GraphQL forbids and graphql-core refuses)."""
        out = []
        for f in d.fields:
            if self.keeps_field(name, f):
                for a in f.arguments or ():
                    self._need_leaf(named(a.type))
                self._need_leaf(named(f.type))
                kept = _plain(f, arguments=tuple(_plain(a) for a in f.arguments or ()))
                out.append(_plain(kept, directives=()) if f.name.value in from_interfaces else kept)
        return tuple(out)

    def pruned(self, d):
        name = d.name.value
        if isinstance(d, (ObjectTypeDefinitionNode, InterfaceTypeDefinitionNode)):
            interfaces = tuple(i for i in d.interfaces or () if i.name.value in self.outputs)
            from_interfaces = {f.name.value for i in interfaces for f in self.defs[i.name.value].fields}
            return _plain(d, fields=self._fields(name, d, from_interfaces), interfaces=interfaces)
        types = tuple(t for t in d.types if t.name.value in self.outputs)
        return _plain(d, types=types) if types else None

    def schema(self) -> DocumentNode:
        self.grow()
        kept = [self.pruned(self.defs[n]) for n in sorted(self.outputs) if n in self.defs]
        kept = [d for d in kept if d is not None]
        leaves = [_leaf(self.defs[n]) for n in sorted(self.leaves)]
        return DocumentNode(definitions=tuple(kept + leaves))

    def extract(self, ours: DocumentNode) -> DocumentNode:
        """GitHub's definitions, whole but without descriptions, of the types in `ours`."""
        return DocumentNode(definitions=tuple(_whole(self.defs[d.name.value]) for d in ours.definitions))


def _directives(node) -> tuple:
    return tuple(x for x in node.directives or () if x.name.value in KEPT_DIRECTIVES)


def _plain(node, **changes):
    """A copy of a definition node without its description and with only the kept directives."""
    kw = {k: getattr(node, k) for k in node.keys if k != "loc"}
    kw.update({"description": None, "directives": _directives(node), **changes})
    return type(node)(**kw)


def _leaf(d):
    if isinstance(d, EnumTypeDefinitionNode):
        drop = OPTIONAL_VALUES.get(d.name.value, set())
        return _plain(d, values=tuple(_plain(v) for v in d.values if v.name.value not in drop))
    if isinstance(d, InputObjectTypeDefinitionNode):
        return _plain(d, fields=tuple(_plain(f) for f in d.fields))
    return _plain(d)


def _whole(d):
    if isinstance(d, (ObjectTypeDefinitionNode, InterfaceTypeDefinitionNode)):
        return _plain(d, fields=tuple(_plain(f, arguments=tuple(_plain(a) for a in f.arguments or ()))
                                      for f in d.fields))
    if isinstance(d, EnumTypeDefinitionNode):
        return _plain(d, values=tuple(_plain(v) for v in d.values))
    if isinstance(d, InputObjectTypeDefinitionNode):
        return _plain(d, fields=tuple(_plain(f) for f in d.fields))
    return _plain(d)


def main(src: str, schema_out: str, extract_out: str) -> None:
    pruner = Pruner(open(src).read())
    ours = pruner.schema()
    open(schema_out, "w").write(print_ast(ours) + "\n")
    open(extract_out, "w").write(print_ast(pruner.extract(ours)) + "\n")


if __name__ == "__main__":
    main(*sys.argv[1:4])
