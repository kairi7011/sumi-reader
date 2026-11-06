# Private source, public distribution

Maintainer changes originate in `kairi7011/sumi-reader-private`. The public `kairi7011/sumi-reader` repository contains reviewed source and media with sanitized history. The repositories have different commit identifiers; compare Git trees to check their contents.

## Before development

Fetch both origins and inspect their main branches. If public has changes that private does not have, first import those commits into the private checkout, review them, and push private main. A cherry-pick from public into private is safe for preserving the public contribution. Do not discard public changes by overwriting them with an older private snapshot.

Implement and test new code, docs, and media in private. Commit in useful increments and push private main before touching the public mirror.

## Before publishing

Run this read-only gate with both clean checkouts on main:

```sh
python3 scripts/check-mirror.py --source "$PRIVATE_CHECKOUT" --public "$PUBLIC_CHECKOUT"
```

The gate checks each HEAD against its live origin main and requires the current public tree to appear in private history. An absent tree means public has changes that need to be incorporated first. If both trees match, no source synchronization is needed.

Review the diff from the reported `incorporated_at` commit to private HEAD. Apply only that audited diff in the public checkout, preserving meaningful commit boundaries when practical. A binary Git patch includes original image assets without copying ignored local files. Use a GitHub noreply author identity for public commits. Never merge private history, push all refs, or mirror the private repository.

Before pushing public, run its tests and `python3 scripts/check-public.py --history`; inspect new screenshots and recordings for private material. After pushing, rerun the mirror gate and verify `identical: true`. Public releases must point to this reviewed public commit. Package the app and source archive using [distribution.md](distribution.md), then verify uploaded checksums.

Do not resolve a failed gate by changing its baseline or force-pushing public. First reconcile the missing source changes in private. If private-only tracked files are needed in the future, define and review an explicit export manifest rather than silently relaxing tree comparison.
