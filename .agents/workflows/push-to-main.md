---
description: push to main and watch GitHub Actions CI and Docs deployment until both pass or fail
---

# Push to Main & Watch Deployment

Push the current branch to `main` and observe both the CI and the Docs GitHub Actions runs in real time using the `gh` CLI. Do not proceed to the next step until the previous one succeeds.

// turbo-all

1. Ensure the working tree is clean and all tests pass:
```bash
uv run pytest
```

2. Push to main:
```bash
git push origin main
```

3. Give GitHub ~5 s to register the new runs, then watch the CI run live:
```bash
sleep 5 && gh run watch $(gh run list --workflow=ci.yml --branch main --limit 1 --json databaseId --jq '.[0].databaseId')
```

4. Print the final CI status (non-zero exit = failure):
```bash
gh run view $(gh run list --workflow=ci.yml --branch main --limit 1 --json databaseId --jq '.[0].databaseId') --exit-status
```

If step 4 fails, inspect the CI logs with:
```bash
gh run view $(gh run list --workflow=ci.yml --branch main --limit 1 --json databaseId --jq '.[0].databaseId') --log-failed
```

5. Check whether a Docs deployment was triggered by the push (the "Deploy Docs" workflow only runs when `docs/**` or `mkdocs.yml` changed):
```bash
gh run list --workflow=docs.yml --branch main --limit 1 --json databaseId,createdAt,status --jq '.[0]'
```

6. If a Docs run was triggered (step 5 returned a run created within the last few minutes), watch it live:
```bash
gh run watch $(gh run list --workflow=docs.yml --branch main --limit 1 --json databaseId --jq '.[0].databaseId')
```

7. Print the final Docs deployment status:
```bash
gh run view $(gh run list --workflow=docs.yml --branch main --limit 1 --json databaseId --jq '.[0].databaseId') --exit-status
```

If step 7 fails, inspect the Docs logs with:
```bash
gh run view $(gh run list --workflow=docs.yml --branch main --limit 1 --json databaseId --jq '.[0].databaseId') --log-failed
```
