# Sumi development

The private repository `kairi7011/sumi-reader-private` is the development source of truth. The public repository `kairi7011/sumi-reader` is an audited distribution mirror.

- Implement, test, commit, and push maintainer changes in the private checkout first. This includes documentation and promotional assets.
- Before starting work or publishing, compare the public HEAD tree with private history. If the public tree is absent, import and review the public changes in private first. Preserve contributors' attribution.
- Run `scripts/check-mirror.py --source <private-checkout> --public <public-checkout>` after fetching both origins. It refuses publication when public contains a tree not incorporated in private or private HEAD has not been pushed.
- Sync only reviewed source changes into public, then run the public history and artifact audits. Never merge private history, mirror private refs, or push private backup branches/tags into public.
- Do not publish a release whose source has not already been committed and pushed privately.

See `docs/maintainer-sync.md` for the procedure. Public contributors can still work in forks and open pull requests normally; maintainers incorporate accepted contributions privately before publishing the next release.
