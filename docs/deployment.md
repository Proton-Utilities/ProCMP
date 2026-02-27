# Deployment

## Prerequisites

- A GitHub repository
- A GitHub token with the right permissions (see below)

---

## Token setup

Add your token to an `.env` file at your project root:

```dotenv title=".env"
GITHUB_TOKEN="ghp_..."
```

!!! caution
    Never commit your `.env` file. Make sure it is in `.gitignore`.

### Fine-grained PAT (recommended)

When creating the token at [github.com/settings/tokens](https://github.com/settings/tokens), grant:

| Permission | Access |
|------------|--------|
| `Contents` | Read and write |
| `Metadata` | Read-only |

### Classic PAT

Grant the `repo` scope.

---

## Config

Add a `deployment` section to your `.pcmp.json`:

```json
{
    "deployment": {
        "github": {
            "owner": "your-username",
            "repo":  "your-repo",
            "tokenEnvVar": "GITHUB_TOKEN"
        }
    }
}
```

`tokenEnvVar` defaults to `GITHUB_TOKEN` if omitted. Set it to a different name if you use multiple tokens.

Enable deploy on a build config with `promptDeploy: true`:

```json
{
    "name": "Release",
    "promptDeploy": true,
    "prerelease": false,
    ...
}
```