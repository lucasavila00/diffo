# Releasing Diffo

Stable releases use bare `<major>.<minor>.<patch>` versions such as `0.0.7`. Do not
prefix the version with `v`, and never reuse a version.

Commit the intended release state and run the repository validation:

```sh
make all
```

Then push the current commit to `main` and create its remote release tag in one
command, replacing `0.0.7` with the new version:

```sh
git push origin main HEAD:refs/tags/0.0.7
```

The tag push starts the release workflow. The refspec creates the tag directly on
GitHub, so no local `git tag` command is required.
