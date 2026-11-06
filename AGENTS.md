# Sumi development

The private repository `kairi7011/sumi-reader-private` is the development source of truth. The public repository `kairi7011/sumi-reader` is an audited distribution mirror.

- Implement, test, commit, and push maintainer changes in the private checkout first. This includes documentation and promotional assets.
- Before starting work or publishing, compare the public HEAD tree with private history. If the public tree is absent, import and review the public changes in private first. Preserve contributors' attribution.
- Run `scripts/check-mirror.py --source <private-checkout> --public <public-checkout>` after fetching both origins. It refuses publication when public contains a tree not incorporated in private or private HEAD has not been pushed.
- Sync only reviewed source changes into public, then run the public history and artifact audits. Never merge private history, mirror private refs, or push private backup branches/tags into public.
- Do not publish a release whose source has not already been committed and pushed privately.
- When delivering an app update on the maintainer's Mac, also replace `/Applications/Sumi.app` with the verified build and check that it launches from that location. A release upload or a build left in `dist` does not finish the local update.
- Preserve the existing library, reading positions, bookmarks, and shared preferences. Keep the `com.kairi7011.sumi` bundle identity and user-data locations stable; never reset or replace user data with demo fixtures during an update. Follow the local update procedure in `docs/distribution.md`.
- Preserve ordered tabs and the selected book or Home across normal quit/relaunch. Work that takes 400 ms or longer must have animated feedback; keep slow file/PDF work off the UI thread where possible, and clear feedback on completion, cancellation, and failure.

See `docs/maintainer-sync.md` for the procedure. Public contributors can still work in forks and open pull requests normally; maintainers incorporate accepted contributions privately before publishing the next release.
