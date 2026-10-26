#!/usr/bin/env python3
"""Apply the Duo flows in .gitlab/duo/ to the GitLab project: the repository is the source of truth.

For each flow in .gitlab/duo/flows.json: create it in the AI Catalog if missing,
otherwise push the YAML from the repo as a new patch version (released versions
are frozen, so a change becomes a new version, never an edit); then make sure it
is enabled on the project with its triggers and conditions.

Mechanism: GitLab GraphQL (aiCatalogItems / aiCatalogFlowCreate /
aiCatalogFlowUpdate / aiCatalogItemConsumerCreate), as used by public
Transcend entries. No token is ever stored in this repo.

Usage:
  GITLAB_TOKEN=<token with api scope> python3 deploy/duo_sync.py [--dry-run] [--check]
    [--project <gitlab project full path>] [--api https://gitlab.com/api/graphql]

Project resolution: --project > GITLAB_PROJECT env > .gitlab/duo/flows.json
"project". If the manifest still holds the REPLACE-ME placeholder, the
script refuses and tells you what to set.
"""
import json
import os
import sys
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
DUO = os.path.join(HERE, "..", ".gitlab", "duo")
DRY = "--dry-run" in sys.argv
CHECK = "--check" in sys.argv


def arg_value(flag):
    if flag in sys.argv:
        i = sys.argv.index(flag)
        if i + 1 < len(sys.argv):
            return sys.argv[i + 1]
    return None


def gql(api, token, query, variables=None):
    req = urllib.request.Request(
        api,
        data=json.dumps({"query": query, "variables": variables or {}}).encode(),
        headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"},
    )
    out = json.load(urllib.request.urlopen(req, timeout=60))
    if out.get("errors"):
        raise SystemExit(f"GraphQL error: {out['errors']}")
    return out["data"]


def fail_on(payload, what):
    if payload.get("errors"):
        raise SystemExit(f"{what}: {payload['errors']}")


def main():
    manifest_path = os.path.join(DUO, "flows.json")
    manifest = json.load(open(manifest_path, encoding="utf-8"))

    # Local validation first, and --check stops here: it must work with
    # no project and no token (the CI validate job runs it on every push).
    import yaml  # PyYAML; CI installs it (see .gitlab-ci.yml validate job).

    for f in manifest["flows"]:
        path = os.path.join(DUO, "flows", f["file"])
        doc = yaml.safe_load(open(path, encoding="utf-8"))
        for key in ("version", "components", "prompts", "routers", "flow"):
            if key not in doc:
                raise SystemExit(f"{f['file']}: missing top-level key {key!r}")
        if doc.get("version") != "v1":
            raise SystemExit(f"{f['file']}: unexpected version {doc.get('version')!r}")
        print(f"{f['name']}: local schema OK ({f['file']})")
    if CHECK:
        print("Check only: all flow files parse and carry the required keys.")
        return

    project = arg_value("--project") or os.environ.get("GITLAB_PROJECT") or manifest.get("project", "")
    if not project or project.startswith("REPLACE-ME"):
        raise SystemExit(
            "No GitLab project path set. Pass --project <group/subgroup/project>, "
            "set GITLAB_PROJECT, or edit .gitlab/duo/flows.json \"project\"."
        )
    api = arg_value("--api") or os.environ.get("GITLAB_API", "https://gitlab.com/api/graphql")
    token = os.environ.get("GITLAB_TOKEN")
    if not token:
        raise SystemExit("GITLAB_TOKEN is not set (a token with api scope; never commit it).")

    proj = gql(api, token, "query($p: ID!) { project(fullPath: $p) { id } }", {"p": project})["project"]
    if not proj:
        raise SystemExit(f"project not found or not visible: {project}")
    pid = proj["id"]
    items = gql(
        api, token,
        "query($p: ID!) { project(fullPath: $p) { aiCatalogItems(itemTypes: [FLOW], first: 100) "
        "{ nodes { id name latestVersion { versionName ... on AiCatalogFlowVersion { definition } } } } } }",
        {"p": project},
    )["project"]["aiCatalogItems"]["nodes"]
    by_name = {i["name"]: i["id"] for i in items}
    current = {i["name"]: (i.get("latestVersion") or {}) for i in items}

    for f in manifest["flows"]:
        definition = open(os.path.join(DUO, "flows", f["file"]), encoding="utf-8").read()
        iid = by_name.get(f["name"])
        if iid is None:
            print(f"{f['name']}: create")
            if DRY:
                continue
            r = gql(
                api, token,
                "mutation($in: AiCatalogFlowCreateInput!) { aiCatalogFlowCreate(input: $in) { errors item { id } } }",
                {"in": {"projectId": pid, "name": f["name"], "description": f["description"],
                        "visibility": "PRIVATE", "release": True, "definition": definition}},
            )["aiCatalogFlowCreate"]
            fail_on(r, f["name"])
            iid = r["item"]["id"]
        elif (current[f["name"]].get("definition") or "").strip() == definition.strip():
            print(f"{f['name']}: unchanged, version {current[f['name']].get('versionName')} ({iid})")
        else:
            # A released version is frozen: the change goes in as a new released patch version.
            print(f"{f['name']}: new version from the repo ({iid}, was {current[f['name']].get('versionName')})")
            if not DRY:
                r = gql(
                    api, token,
                    "mutation($in: AiCatalogFlowUpdateInput!) { aiCatalogFlowUpdate(input: $in) { errors } }",
                    {"in": {"id": iid, "definition": definition, "release": True, "versionBump": "PATCH"}},
                )["aiCatalogFlowUpdate"]
                fail_on(r, f["name"])
        consumer = None
        if not (DRY and iid is None):
            consumer = gql(
                api, token,
                "query($p: ID!, $i: AiCatalogItemID!) { project(fullPath: $p) { "
                "aiCatalogItemConsumerForItem(id: $i) { id } } }",
                {"p": project, "i": iid},
            )["project"]["aiCatalogItemConsumerForItem"]
        if consumer:
            print(f"  enabled ({consumer['id']}), triggers {f['triggers']}")
            continue
        print(f"  enable with triggers {f['triggers']}")
        if DRY:
            continue
        inp = {"itemId": iid, "target": {"projectId": pid}, "triggerTypes": f["triggers"]}
        if f.get("conditions"):
            inp["triggerConditions"] = f["conditions"]
        r = gql(
            api, token,
            "mutation($in: AiCatalogItemConsumerCreateInput!) { aiCatalogItemConsumerCreate(input: $in) "
            "{ errors itemConsumer { id } } }",
            {"in": inp},
        )["aiCatalogItemConsumerCreate"]
        fail_on(r, f["name"] + " enable")
        print(f"  enabled ({r['itemConsumer']['id']})")


if __name__ == "__main__":
    main()
